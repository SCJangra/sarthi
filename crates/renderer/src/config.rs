use crate::{CursorMotion, Motion};

#[derive(Debug, Clone)]
pub struct RendererConfig {
	/// Color the window is cleared to, as `0xRRGGBB` in sRGB, until [`crate::Renderer::set_clear_color`]
	/// changes it
	pub background: u32,
	pub power_preference: wgpu::PowerPreference,
	pub present_mode: Option<wgpu::PresentMode>,
	pub prefer_srgb_surface_format: bool,
	pub desired_maximum_frame_latency: u32,

	/// How text that scrolled moves to its new place, until [`crate::Renderer::set_scroll_motion`] changes it
	pub scroll: Motion,

	/// How the cursor moves to a new place, until [`crate::Renderer::set_cursor_motion`] changes it
	pub cursor: CursorMotion,
}

impl Default for RendererConfig {
	fn default() -> Self {
		Self {
			background: 0x000000,
			power_preference: wgpu::PowerPreference::default(),
			present_mode: Some(wgpu::PresentMode::Fifo),
			prefer_srgb_surface_format: true,
			desired_maximum_frame_latency: 2,
			scroll: Motion::Spring { stiffness: 400.0, damping: 32.0 },
			cursor: CursorMotion::default(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_values() {
		let config = RendererConfig::default();

		assert_eq!(config.background, 0x000000);
		assert_eq!(config.power_preference, wgpu::PowerPreference::default());
		assert_eq!(config.present_mode, Some(wgpu::PresentMode::Fifo));
		assert!(config.prefer_srgb_surface_format);
		assert_eq!(config.desired_maximum_frame_latency, 2);
	}
}
