//! How a renderer is set up: the surface's size and format

use smart_default::SmartDefault;

use crate::{CursorMotion, Motion};

/// How a [`crate::Renderer`] sets up the GPU and the window's surface
#[derive(Debug, Clone, SmartDefault)]
pub struct RendererConfig {
	/// Color the window is cleared to, as `0xRRGGBB` in sRGB, until [`crate::Renderer::set_clear_color`]
	/// changes it
	pub background: u32,
	/// Which GPU to use, when there are several
	pub power_preference: wgpu::PowerPreference,
	/// How frames are presented, if not the best one the surface supports
	#[default(Some(wgpu::PresentMode::Fifo))]
	pub present_mode: Option<wgpu::PresentMode>,
	/// Whether to draw to an sRGB surface, which blends colors as they look
	#[default(true)]
	pub prefer_srgb_surface_format: bool,
	/// How many frames may be queued for the display: fewer show input sooner
	#[default(2)]
	pub desired_maximum_frame_latency: u32,

	/// How text that scrolled moves to its new place, until [`crate::Renderer::set_scroll_motion`] changes it
	#[default(Motion::Spring { stiffness: 400.0, damping: 32.0 })]
	pub scroll: Motion,

	/// How the cursor moves to a new place, until [`crate::Renderer::set_cursor_motion`] changes it
	pub cursor: CursorMotion,

	/// How opaque the window's background is, from 0 to 1, until [`crate::Renderer::set_opacity`] changes it
	#[default(1.0)]
	pub opacity: f32,
}
