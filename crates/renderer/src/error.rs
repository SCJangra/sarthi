//! What can go wrong drawing a frame.

/// What can go wrong setting up the renderer or drawing a frame.
#[derive(Debug, thiserror::Error)]
pub enum RendererError {
	/// The window's surface couldn't be made.
	#[error("CreateSurface({0})")]
	CreateSurface(#[from] wgpu::CreateSurfaceError),

	/// No GPU can draw to the window's surface.
	#[error("NoAdapter")]
	NoAdapter,

	/// The GPU couldn't be set up.
	#[error("RequestDevice({0})")]
	RequestDevice(#[from] wgpu::RequestDeviceError),

	/// The surface takes no color format the renderer can draw in.
	#[error("NoSurfaceFormat")]
	NoSurfaceFormat,

	/// The surface has no way to present frames.
	#[error("NoPresentMode")]
	NoPresentMode,

	/// The surface can't present frames the way [`crate::RendererConfig::present_mode`] asks.
	#[error("UnsupportedPresentMode({0:?})")]
	UnsupportedPresentMode(wgpu::PresentMode),

	/// The surface can't be composited with the desktop.
	#[error("NoAlphaMode")]
	NoAlphaMode,

	/// The surface must be recreated, e.g. with a new [`crate::Renderer`].
	#[error("SurfaceLost")]
	SurfaceLost,

	/// The surface's configuration is invalid, a bug in the renderer.
	#[error("SurfaceValidation")]
	SurfaceValidation,

	/// Text couldn't be prepared for drawing, e.g. the glyph atlas is full.
	#[error("PrepareText({0})")]
	PrepareText(#[from] glyphon::PrepareError),

	/// Text couldn't be drawn.
	#[error("RenderText({0})")]
	RenderText(#[from] glyphon::RenderError),
}
