use winit::dpi::LogicalSize;

#[derive(Debug, Clone)]
pub struct WindowConfig {
	pub title: String,
	pub width: u32,
	pub height: u32,
	pub resizable: bool,

	/// Size of one grid cell, in logical pixels.
	///
	/// TODO: Derive this from the font's metrics once text rendering exists. Until then it is a
	/// fixed size, used to tell Neovim how many columns and rows fit in the window.
	pub cell_size: LogicalSize<f64>,
}

impl Default for WindowConfig {
	fn default() -> Self {
		Self {
			title: String::from("NvUI"),
			width: 1280,
			height: 720,
			resizable: true,
			cell_size: LogicalSize::new(9.0, 18.0),
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
		assert_eq!(config.cell_size, LogicalSize::new(9.0, 18.0));
	}
}
