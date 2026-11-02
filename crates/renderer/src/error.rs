#[derive(Debug, thiserror::Error)]
pub enum RendererError {
	#[error("CreateSurface({0})")]
	CreateSurface(#[from] wgpu::CreateSurfaceError),

	#[error("NoAdapter")]
	NoAdapter,

	#[error("RequestDevice({0})")]
	RequestDevice(#[from] wgpu::RequestDeviceError),

	#[error("NoSurfaceFormat")]
	NoSurfaceFormat,

	#[error("NoPresentMode")]
	NoPresentMode,

	#[error("UnsupportedPresentMode({0:?})")]
	UnsupportedPresentMode(wgpu::PresentMode),

	#[error("NoAlphaMode")]
	NoAlphaMode,

	/// The surface must be recreated, e.g. with a new [`crate::Renderer`]
	#[error("SurfaceLost")]
	SurfaceLost,

	#[error("SurfaceValidation")]
	SurfaceValidation,
}
