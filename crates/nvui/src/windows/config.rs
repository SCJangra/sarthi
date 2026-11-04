#[derive(Debug, Clone)]
pub struct WindowConfig {
	pub title: String,
	pub width: u32,
	pub height: u32,
	pub resizable: bool,

	/// Font size, in logical pixels
	pub font_size: f32,
}

impl Default for WindowConfig {
	fn default() -> Self {
		Self {
			title: String::from("NvUI"),
			width: 1280,
			height: 720,
			resizable: true,
			font_size: 14.0,
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
