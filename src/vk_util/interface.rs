use super::*;

pub use vulkano::command_buffer::AutoCommandBufferBuilder;
pub use vulkano::device::{Device, Queue};
pub use vulkano::format::Format;
pub use vulkano::image::StorageImage;

use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::device::{DeviceExtensions, Features};
use vulkano::instance::{Instance, InstanceExtensions, PhysicalDevice};

pub struct Interface {
	device: Arc<Device>,
	queue: Arc<Queue>,
	info: String,
}

impl Interface {
	pub fn new_compute() -> Self {
		let instance = Self::init_instance();
		let physical = Self::init_physical(&instance);
		let info = format!("{} ({:?})", physical.name(), physical.ty());
		let (device, queue) = Self::init_device_queue(physical);
		Self { device, queue, info }
	}

	pub fn info(&self) -> &str {
		&self.info
	}

	pub fn device(&self) -> Arc<Device> {
		self.device.clone()
	}

	pub fn queue(&self) -> Arc<Queue> {
		self.queue.clone()
	}

	pub fn storage_image<D: Into<UVec2>>(&self, dim: D, format: Format) -> Arc<StorageImage<Format>> {
		let dim: UVec2 = dim.into();
		StorageImage::new(self.device(), dim.into(), format, Some(self.queue.family())).unwrap()
	}

	pub fn cpu_accessible_buffer(&self, size: usize) -> Arc<CpuAccessibleBuffer<[u8]>> {
		self.cpu_accessible_buffer_from((0..size).map(|_| 0u8))
	}

	pub fn cpu_accessible_buffer_from<I>(&self, data: I) -> Arc<CpuAccessibleBuffer<[u8]>>
	where
		I: ExactSizeIterator<Item = u8>,
	{
		CpuAccessibleBuffer::from_iter(self.device(), BufferUsage::all(), false, data).unwrap()
	}

	pub fn auto_command_buffer_builder(&self) -> AutoCommandBufferBuilder {
		AutoCommandBufferBuilder::new(self.device(), self.queue.family()).unwrap()
	}

	fn init_instance() -> Arc<Instance> {
		Instance::new(None, &InstanceExtensions::none(), None).expect("create vulkan instance")
	}

	fn init_physical(instance: &Arc<Instance>) -> PhysicalDevice {
		PhysicalDevice::enumerate(instance).next().expect("no vulkan device available")
	}

	fn init_device_queue(physical: PhysicalDevice) -> (Arc<Device>, Arc<Queue>) {
		let queue_family = physical.queue_families().find(|&q| q.supports_graphics()).unwrap();

		let (device, mut queues) = {
			Device::new(
				physical,
				&Features::none(),
				&DeviceExtensions::none(),
				[(queue_family, 0.5)].iter().cloned(),
			)
			.unwrap()
		};

		let queue = queues.next().unwrap();
		(device, queue)
	}
}
