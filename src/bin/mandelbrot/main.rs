// Based on https://github.com/vulkano-rs/vulkano-www/blob/master/examples/guide-mandelbrot.rs
// which carries this notice:
//
// Copyright (c) 2017 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use image::ImageBuffer;
use image::Rgba;
use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DynamicState};
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor::pipeline_layout::PipelineLayoutAbstract;
use vulkano::device::{Device, DeviceExtensions, Features, Queue};
use vulkano::format::Format;
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{Dimensions, ImageUsage, StorageImage, SwapchainImage};
use vulkano::instance::{Instance, InstanceExtensions, PhysicalDevice, QueueFamily};
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline};
use vulkano::swapchain;
use vulkano::swapchain::{AcquireError, ColorSpace, FullscreenExclusive, PresentMode, Surface, SurfaceTransform, Swapchain, SwapchainCreationError};
use vulkano::sync::{FlushError, GpuFuture};
use vulkano_win::VkSurfaceBuild;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

fn main() {
	let instance = Instance::new(None, &InstanceExtensions::none(), None).expect("failed to create instance");

	let physical = PhysicalDevice::enumerate(&instance).next().expect("no device available");

	let queue_family = physical
		.queue_families()
		.find(|&q| q.supports_graphics())
		.expect("couldn't find a graphical queue family");

	let (device, mut queues) = {
		Device::new(
			physical,
			&Features::none(),
			&DeviceExtensions::none(),
			[(queue_family, 0.5)].iter().cloned(),
		)
		.expect("failed to create device")
	};

	let queue = queues.next().unwrap();

	let image = StorageImage::new(
		device.clone(),
		Dimensions::Dim2d { width: 1024, height: 1024 },
		Format::R8G8B8A8Unorm,
		Some(queue.family()),
	)
	.unwrap();

	mod cs {
		vulkano_shaders::shader! {
			ty: "compute",
			src: "
#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0, rgba8) uniform writeonly image2D img;

void main() {
    vec2 norm_coordinates = (gl_GlobalInvocationID.xy + vec2(0.5)) / vec2(imageSize(img));

    vec2 c = (norm_coordinates - vec2(0.5)) * 2.0 - vec2(1.0, 0.0);

    vec2 z = vec2(0.0, 0.0);
    float i;
    for (i = 0.0; i < 1.0; i += 0.005) {
        z = vec2(
            z.x * z.x - z.y * z.y + c.x,
            z.y * z.x + z.x * z.y + c.y
        );

        if (length(z) > 4.0) {
            break;
        }
    }

    vec4 to_write = vec4(vec3(i), 1.0);
    imageStore(img, ivec2(gl_GlobalInvocationID.xy), to_write);
}"
		}
	}

	let shader = cs::Shader::load(device.clone()).expect("failed to create shader module");

	let compute_pipeline =
		Arc::new(ComputePipeline::new(device.clone(), &shader.main_entry_point(), &()).expect("failed to create compute pipeline"));

	let set = Arc::new(
		PersistentDescriptorSet::start(compute_pipeline.layout().descriptor_set_layout(0).unwrap().clone())
			.add_image(image.clone())
			.unwrap()
			.build()
			.unwrap(),
	);

	let buf = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), false, (0..1024 * 1024 * 4).map(|_| 0u8))
		.expect("failed to create buffer");

	let mut builder = AutoCommandBufferBuilder::new(device.clone(), queue.family()).unwrap();
	builder
		.dispatch([1024 / 8, 1024 / 8, 1], compute_pipeline.clone(), set.clone(), ())
		.unwrap()
		.copy_image_to_buffer(image.clone(), buf.clone())
		.unwrap();
	let command_buffer = builder.build().unwrap();

	let finished = command_buffer.execute(queue.clone()).unwrap();
	finished.then_signal_fence_and_flush().unwrap().wait(None).unwrap();

	let buffer_content = buf.read().unwrap();
	let image = ImageBuffer::<Rgba<u8>, _>::from_raw(1024, 1024, &buffer_content[..]).unwrap();
	image.save("image.png").unwrap();
}