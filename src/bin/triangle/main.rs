// Based on https://github.com/vulkano-rs/vulkano-examples/blob/master/src/bin/triangle.rs
// which carries this notice:
//
// Copyright (c) 2016 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

// Welcome to the triangle example!
//
// This is the only example that is entirely detailed. All the other examples avoid code
// duplication by using helper functions.
//
// This example assumes that you are already more or less familiar with graphics programming
// and that you want to learn Vulkan. This means that for example it won't go into details about
// what a vertex or a shader is.

use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::device::Queue;
use vulkano::device::{Device, DeviceExtensions};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, PhysicalDevice, QueueFamily};
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::swapchain;
use vulkano::swapchain::{AcquireError, ColorSpace, FullscreenExclusive, PresentMode, Surface, SurfaceTransform, Swapchain, SwapchainCreationError};
use vulkano::sync;
use vulkano::sync::{FlushError, GpuFuture};

use vulkano_win::VkSurfaceBuild;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use std::sync::Arc;

fn main() {
	let instance = instance_win();
	let (event_loop, surface) = window(&instance, "vulkan playground");
	let physical = physical(&instance);
	let queue_family = rendering_queue_family(physical, surface.clone());
	let (device, queue) = device(physical, queue_family);
	let (mut swapchain, images) = swapchain(physical, device.clone(), queue.clone(), surface.clone());

	// We now create a buffer that will store the shape of our triangle.
	let vertex_buffer = {
		#[derive(Default, Debug, Clone)]
		struct Vertex {
			position: [f32; 2],
		}
		vulkano::impl_vertex!(Vertex, position);

		CpuAccessibleBuffer::from_iter(
			device.clone(),
			BufferUsage::all(),
			false,
			[
				Vertex { position: [-0.5, -0.25] },
				Vertex { position: [0.0, 0.5] },
				Vertex { position: [0.25, -0.1] },
			]
			.iter()
			.cloned(),
		)
		.unwrap()
	};

	// https://docs.rs/vulkano-shaders/
	mod vs {
		vulkano_shaders::shader! {
			ty: "vertex",
			src: "
				#version 450

				layout(location = 0) in vec2 position;

				void main() {
					gl_Position = vec4(position, 0.0, 1.0);
				}
			"
		}
	}

	mod fs {
		vulkano_shaders::shader! {
			ty: "fragment",
			src: "
				#version 450

				layout(location = 0) out vec4 f_color;

				void main() {
					f_color = vec4(1.0, 1.0, 0.0, 1.0);
				}
			"
		}
	}

	let vs = vs::Shader::load(device.clone()).unwrap();
	let fs = fs::Shader::load(device.clone()).unwrap();

	// The next step is to create a *render pass*, which is an object that describes where the
	// output of the graphics pipeline will go. It describes the layout of the images
	// where the colors, depth and/or stencil information will be written.
	let render_pass = Arc::new(
		vulkano::single_pass_renderpass!(
			device.clone(),
			attachments: {
				// `color` is a custom name we give to the first and only attachment.
				color: {
					// `load: Clear` means that we ask the GPU to clear the content of this
					// attachment at the start of the drawing.
					load: Clear,
					// `store: Store` means that we ask the GPU to store the output of the draw
					// in the actual image. We could also ask it to discard the result.
					store: Store,
					// `format: <ty>` indicates the type of the format of the image. This has to
					// be one of the types of the `vulkano::format` module (or alternatively one
					// of your structs that implements the `FormatDesc` trait). Here we use the
					// same format as the swapchain.
					format: swapchain.format(),
					// TODO:
					samples: 1,
				}
			},
			pass: {
				// We use the attachment named `color` as the one and only color attachment.
				color: [color],
				// No depth-stencil attachment is indicated with empty brackets.
				depth_stencil: {}
			}
		)
		.unwrap(),
	);

	// Before we draw we have to create what is called a pipeline. This is similar to an OpenGL
	// program, but much more specific.
	let pipeline = Arc::new(
		GraphicsPipeline::start()
			// We need to indicate the layout of the vertices.
			// The type `SingleBufferDefinition` actually contains a template parameter corresponding
			// to the type of each vertex. But in this code it is automatically inferred.
			.vertex_input_single_buffer()
			// A Vulkan shader can in theory contain multiple entry points, so we have to specify
			// which one. The `main` word of `main_entry_point` actually corresponds to the name of
			// the entry point.
			.vertex_shader(vs.main_entry_point(), ())
			// The content of the vertex buffer describes a list of triangles.
			.triangle_list()
			// Use a resizable viewport set to draw over the entire window
			.viewports_dynamic_scissors_irrelevant(1)
			// See `vertex_shader`.
			.fragment_shader(fs.main_entry_point(), ())
			// We have to indicate which subpass of which render pass this pipeline is going to be used
			// in. The pipeline will only be usable from this particular subpass.
			.render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
			// Now that our builder is filled, we call `build()` to obtain an actual pipeline.
			.build(device.clone())
			.unwrap(),
	);

	// Dynamic viewports allow us to recreate just the viewport when the window is resized
	// Otherwise we would have to recreate the whole pipeline.
	let mut dynamic_state = DynamicState {
		line_width: None,
		viewports: None,
		scissors: None,
		compare_mask: None,
		write_mask: None,
		reference: None,
	};

	// The render pass we created above only describes the layout of our framebuffers. Before we
	// can draw we also need to create the actual framebuffers.
	//
	// Since we need to draw to multiple images, we are going to create a different framebuffer for
	// each image.
	let mut framebuffers = window_size_dependent_setup(&images, render_pass.clone(), &mut dynamic_state);

	// Initialization is finally finished!

	// In some situations, the swapchain will become invalid by itself. This includes for example
	// when the window is resized (as the images of the swapchain will no longer match the
	// window's) or, on Android, when the application went to the background and goes back to the
	// foreground.
	//
	// In this situation, acquiring a swapchain image or presenting it will return an error.
	// Rendering to an image of that swapchain will not produce any error, but may or may not work.
	// To continue rendering, we need to recreate the swapchain by creating a new swapchain.
	// Here, we remember that we need to do this for the next loop iteration.
	let mut recreate_swapchain = false;

	// In the loop below we are going to submit commands to the GPU. Submitting a command produces
	// an object that implements the `GpuFuture` trait, which holds the resources for as long as
	// they are in use by the GPU.
	//
	// Destroying the `GpuFuture` blocks until the GPU is finished executing it. In order to avoid
	// that, we store the submission of the previous frame here.
	let mut previous_frame_end = Some(sync::now(device.clone()).boxed());

	event_loop.run(move |event, _, control_flow| {
		match event {
			Event::WindowEvent {
				event: WindowEvent::CloseRequested,
				..
			} => {
				*control_flow = ControlFlow::Exit;
			}
			Event::WindowEvent {
				event: WindowEvent::Resized(_),
				..
			} => {
				recreate_swapchain = true;
			}
			Event::RedrawEventsCleared => {
				// It is important to call this function from time to time, otherwise resources will keep
				// accumulating and you will eventually reach an out of memory error.
				// Calling this function polls various fences in order to determine what the GPU has
				// already processed, and frees the resources that are no longer needed.
				previous_frame_end.as_mut().unwrap().cleanup_finished();

				// Whenever the window resizes we need to recreate everything dependent on the window size.
				// In this example that includes the swapchain, the framebuffers and the dynamic state viewport.
				if recreate_swapchain {
					// Get the new dimensions of the window.
					let dimensions: [u32; 2] = surface.window().inner_size().into();
					let (new_swapchain, new_images) = match swapchain.recreate_with_dimensions(dimensions) {
						Ok(r) => r,
						// This error tends to happen when the user is manually resizing the window.
						// Simply restarting the loop is the easiest way to fix this issue.
						Err(SwapchainCreationError::UnsupportedDimensions) => return,
						Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
					};

					swapchain = new_swapchain;
					// Because framebuffers contains an Arc on the old swapchain, we need to
					// recreate framebuffers as well.
					framebuffers = window_size_dependent_setup(&new_images, render_pass.clone(), &mut dynamic_state);
					recreate_swapchain = false;
				}

				// Before we can draw on the output, we have to *acquire* an image from the swapchain. If
				// no image is available (which happens if you submit draw commands too quickly), then the
				// function will block.
				// This operation returns the index of the image that we are allowed to draw upon.
				//
				// This function can block if no image is available. The parameter is an optional timeout
				// after which the function call will return an error.
				let (image_num, suboptimal, acquire_future) = match swapchain::acquire_next_image(swapchain.clone(), None) {
					Ok(r) => r,
					Err(AcquireError::OutOfDate) => {
						recreate_swapchain = true;
						return;
					}
					Err(e) => panic!("Failed to acquire next image: {:?}", e),
				};

				// acquire_next_image can be successful, but suboptimal. This means that the swapchain image
				// will still work, but it may not display correctly. With some drivers this can be when
				// the window resizes, but it may not cause the swapchain to become out of date.
				if suboptimal {
					recreate_swapchain = true;
				}

				// Specify the color to clear the framebuffer with i.e. blue
				let clear_values = vec![[0.0, 0.0, 1.0, 1.0].into()];

				// In order to draw, we have to build a *command buffer*. The command buffer object holds
				// the list of commands that are going to be executed.
				//
				// Building a command buffer is an expensive operation (usually a few hundred
				// microseconds), but it is known to be a hot path in the driver and is expected to be
				// optimized.
				//
				// Note that we have to pass a queue family when we create the command buffer. The command
				// buffer will only be executable on that given queue family.
				let mut builder = AutoCommandBufferBuilder::primary_one_time_submit(device.clone(), queue.family()).unwrap();

				builder
					// Before we can draw, we have to *enter a render pass*. There are two methods to do
					// this: `draw_inline` and `draw_secondary`. The latter is a bit more advanced and is
					// not covered here.
					//
					// The third parameter builds the list of values to clear the attachments with. The API
					// is similar to the list of attachments when building the framebuffers, except that
					// only the attachments that use `load: Clear` appear in the list.
					.begin_render_pass(framebuffers[image_num].clone(), false, clear_values)
					.unwrap()
					// We are now inside the first subpass of the render pass. We add a draw command.
					//
					// The last two parameters contain the list of resources to pass to the shaders.
					// Since we used an `EmptyPipeline` object, the objects have to be `()`.
					.draw(pipeline.clone(), &dynamic_state, vertex_buffer.clone(), (), ())
					.unwrap()
					// We leave the render pass by calling `draw_end`. Note that if we had multiple
					// subpasses we could have called `next_inline` (or `next_secondary`) to jump to the
					// next subpass.
					.end_render_pass()
					.unwrap();

				// Finish building the command buffer by calling `build`.
				let command_buffer = builder.build().unwrap();

				let future = previous_frame_end
					.take()
					.unwrap()
					.join(acquire_future)
					.then_execute(queue.clone(), command_buffer)
					.unwrap()
					// The color output is now expected to contain our triangle. But in order to show it on
					// the screen, we have to *present* the image by calling `present`.
					//
					// This function does not actually present the image immediately. Instead it submits a
					// present command at the end of the queue. This means that it will only be presented once
					// the GPU has finished executing the command buffer that draws the triangle.
					.then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
					.then_signal_fence_and_flush();

				match future {
					Ok(future) => {
						previous_frame_end = Some(future.boxed());
					}
					Err(FlushError::OutOfDate) => {
						recreate_swapchain = true;
						previous_frame_end = Some(sync::now(device.clone()).boxed());
					}
					Err(e) => {
						println!("Failed to flush future: {:?}", e);
						previous_frame_end = Some(sync::now(device.clone()).boxed());
					}
				}
			}
			_ => (),
		}
	});
}

// Initialize a vulkan instance capable of drawing to a window.
// TODO: return result.
fn instance_win() -> Arc<Instance> {
	let required_extensions = vulkano_win::required_extensions();
	let instance = Instance::new(None, &required_extensions, None).unwrap();
	println!("vulkan instance: {:?}", &instance);
	instance
}

// Find and initialze a vulkan physical device.
// TODO: pick suitable device when multiple are available
fn physical(instance: &Arc<Instance>) -> PhysicalDevice {
	let physical = PhysicalDevice::enumerate(instance).next().unwrap();
	println!("physical device: {} ({:?})", physical.name(), physical.ty());
	physical
}

// Create a new window and return its event loop and drawable surface.
fn window(instance: &Arc<Instance>, title: &str) -> (EventLoop<()>, Arc<Surface<Window>>) {
	let event_loop = EventLoop::new();
	let surface = WindowBuilder::new()
		.with_title(title)
		.build_vk_surface(&event_loop, instance.clone())
		.unwrap();
	println!("created window & event loop: {}", title);
	(event_loop, surface)
}

// Find the first queue that supports drawing to `surface`.
// TODO: provide a separte queue for transfers.
fn rendering_queue_family(physical: PhysicalDevice, surface: Arc<Surface<Window>>) -> QueueFamily {
	let queue_family = physical
		.queue_families()
		.find(|&q| q.supports_graphics() && surface.is_supported(q).unwrap_or(false))
		.unwrap();
	println!("created queue family: {:?}", &queue_family);
	queue_family
}

fn device(physical: PhysicalDevice, queue_family: QueueFamily) -> (Arc<Device>, Arc<Queue>) {
	let device_ext = DeviceExtensions {
		khr_swapchain: true,
		..DeviceExtensions::none()
	};
	let (device, mut queues) = Device::new(
		physical,
		physical.supported_features(),
		&device_ext,
		[(queue_family, 0.5)].iter().cloned(),
	)
	.unwrap();
	println!("created (virtual) device: {:?}", &device);
	// Since we can request multiple queues, the `queues` variable is in fact an iterator. In this
	// example we use only one queue, so we just retrieve the first and only element of the
	// iterator and throw it away.
	let queue = queues.next().unwrap();
	println!("selected queue: {:?}", &queue);
	(device, queue)
}

fn swapchain(
	physical: PhysicalDevice,
	device: Arc<Device>,
	queue: Arc<Queue>,
	surface: Arc<Surface<Window>>,
) -> (Arc<Swapchain<Window>>, Vec<Arc<SwapchainImage<Window>>>) {
	// Querying the capabilities of the surface. When we create the swapchain we can only
	// pass values that are allowed by the capabilities.
	let caps = surface.capabilities(physical).unwrap();

	// The alpha mode indicates how the alpha value of the final image will behave. For example
	// you can choose whether the window will be opaque or transparent.
	let alpha = caps.supported_composite_alpha.iter().next().unwrap();

	// Choosing the internal format that the images will have.
	let format = caps.supported_formats[0].0;

	// The dimensions of the window, only used to initially setup the swapchain.
	// NOTE:
	// On some drivers the swapchain dimensions are specified by `caps.current_extent` and the
	// swapchain size must use these dimensions.
	// These dimensions are always the same as the window dimensions
	//
	// However other drivers dont specify a value i.e. `caps.current_extent` is `None`
	// These drivers will allow anything but the only sensible value is the window dimensions.
	//
	// Because for both of these cases, the swapchain needs to be the window dimensions, we just use that.
	let dimensions: [u32; 2] = surface.window().inner_size().into();

	// Please take a look at the docs for the meaning of the parameters we didn't mention.
	println!("creating swapchain");
	println!("  - device: {:?}", &device);
	println!("  - surface: {:?}", &surface);
	println!("  - caps.min_image_count: {}", caps.min_image_count);
	println!("  - format: {:?}", format);
	println!("  - dimensions: {:?}", dimensions);
	println!("  - layers: {:?}", 1);
	println!("  - usage: {:?}", ImageUsage::color_attachment());
	println!("  - queue: {:?}", &queue);
	println!("  - transform: {:?}", SurfaceTransform::Identity);
	println!("  - alpha: {:?}", alpha);
	println!("  - mode: {:?}", PresentMode::Fifo);
	println!("  - fullscreen_exlusive: {:?}", FullscreenExclusive::Default);
	println!("  - clipped: {:?}", true);
	println!("  - color_space {:?}", ColorSpace::SrgbNonLinear);

	Swapchain::new(
		device,
		surface,
		caps.min_image_count,
		format,
		dimensions,
		1,
		ImageUsage::color_attachment(),
		&queue,
		SurfaceTransform::Identity,
		alpha,
		PresentMode::Fifo,
		FullscreenExclusive::Default,
		true,
		ColorSpace::SrgbNonLinear,
	)
	.unwrap()
}

/// This method is called once during initialization, then again whenever the window is resized
fn window_size_dependent_setup(
	images: &[Arc<SwapchainImage<Window>>],
	render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
	dynamic_state: &mut DynamicState,
) -> Vec<Arc<dyn FramebufferAbstract + Send + Sync>> {
	let dimensions = images[0].dimensions();

	let viewport = Viewport {
		origin: [0.0, 0.0],
		dimensions: [dimensions[0] as f32, dimensions[1] as f32],
		depth_range: 0.0..1.0,
	};
	dynamic_state.viewports = Some(vec![viewport]);

	images
		.iter()
		.map(|image| {
			Arc::new(Framebuffer::start(render_pass.clone()).add(image.clone()).unwrap().build().unwrap())
				as Arc<dyn FramebufferAbstract + Send + Sync>
		})
		.collect::<Vec<_>>()
}