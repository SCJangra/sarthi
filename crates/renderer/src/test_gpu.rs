//! One GPU device for the tests that draw. Creating devices from tests running in parallel crashed the driver now and
//! then (SIGSEGV), so the tests share one, one test at a time.

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

pub(crate) struct Gpu {
	pub device: wgpu::Device,
	pub queue: wgpu::Queue,
}

/// The shared device, while no other test uses it, or `None` without a GPU
pub(crate) fn gpu() -> Option<MutexGuard<'static, Gpu>> {
	static GPU: OnceLock<Option<Mutex<Gpu>>> = OnceLock::new();
	let gpu = GPU.get_or_init(|| {
		smol::block_on(async {
			let instance = wgpu::Instance::default();
			let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.ok()?;
			let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.ok()?;
			Some(Mutex::new(Gpu { device, queue }))
		})
	});
	// A test that failed while drawing leaves the device as usable as before.
	Some(gpu.as_ref()?.lock().unwrap_or_else(PoisonError::into_inner))
}
