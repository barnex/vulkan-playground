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
use vulkano::command_buffer::CommandBuffer;
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor::pipeline_layout::PipelineLayoutAbstract;
use vulkano::format::Format;
use vulkano::pipeline::ComputePipeline;
use vulkano::sync::GpuFuture;

use vulkan_playground::*;

fn main() {
	let started = now();

	// init
	let vk = Interface::new_compute();
	println!("using {}", vk.info());

	// buffers
	let (w, h) = (2048, 2048);
	let gpu_image = vk.storage_image((w, h), Format::R8G8B8A8Unorm);
	let cpu_buffer = vk.cpu_accessible_buffer((w * h * 4) as usize);

	// shader
	mod cs {
		vulkano_shaders::shader! {
			ty: "compute",
			// v6
			path: "src/bin/mandelbrot/mandelbrot.glsl",
		}
	}
	let shader = cs::Shader::load(vk.device()).unwrap();

	// command
	let compute_pipeline = Arc::new(ComputePipeline::new(vk.device(), &shader.main_entry_point(), &()).unwrap());
	let set = Arc::new(
		PersistentDescriptorSet::start(compute_pipeline.layout().descriptor_set_layout(0).unwrap().clone())
			.add_image(gpu_image.clone())
			.unwrap()
			.build()
			.unwrap(),
	);

	let local_size_x = 8;
	let local_size_y = 8;
	let local_size_z = 1;

	let mut builder = vk.auto_command_buffer_builder();
	builder
		.dispatch(
			[w / local_size_x, h / local_size_y, local_size_z],
			compute_pipeline.clone(),
			set.clone(),
			(),
		)
		.unwrap()
		.copy_image_to_buffer(gpu_image.clone(), cpu_buffer.clone())
		.unwrap();
	let command_buffer = builder.build().unwrap();
	println!("init: {} ms", started.elapsed().as_secs_f32() * 1000.0);

	// exec + transfer
	let started = now();
	let finished = command_buffer.execute(vk.queue()).unwrap();
	finished.then_signal_fence_and_flush().unwrap().wait(None).unwrap();
	let buffer_content = cpu_buffer.read().unwrap(); // read is really just lock
	println!("compute + transfer: {} ms", started.elapsed().as_secs_f32() * 1000.0);

	let started = now();
	let image = ImageBuffer::<Rgba<u8>, _>::from_raw(w, h, &buffer_content[..]).unwrap();
	image.save("image.png").expect("save image.png");
	println!("encode: {} ms", started.elapsed().as_secs_f32() * 1000.0);
}

fn now() -> std::time::Instant {
	std::time::Instant::now()
}
