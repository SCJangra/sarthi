use renderer::UnderlineSize;

#[derive(Debug, Clone)]
pub struct WindowConfig {
	pub title: String,
	pub width: u32,
	pub height: u32,
	pub resizable: bool,

	/// Font size, in logical pixels
	pub font_size: f32,

	/// How underlines are drawn, in logical pixels: adjustments to the font's position and thickness, the undercurl's
	/// height, and whether they break around the text's ink (off unless set)
	pub underlines: UnderlineSize,

	/// Lines a touchpad scrolls per row of finger movement: 1 follows the fingers, more scrolls faster
	pub touchpad_scroll: f64,
}

impl Default for WindowConfig {
	fn default() -> Self {
		Self {
			title: String::from("NvUI"),
			width: 1280,
			height: 720,
			resizable: true,
			font_size: 14.0,
			underlines: UnderlineSize { offset: 0.0, thickness: None, wave_height: 2.0, ink_gaps: None },
			touchpad_scroll: 3.0,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_values() {
		let config = WindowConfig::default();

		assert_eq!(config.title, "NvUI");
		assert_eq!(config.width, 1280);
		assert_eq!(config.height, 720);
		assert!(config.resizable);
		assert_eq!(config.font_size, 14.0);
	}
}
