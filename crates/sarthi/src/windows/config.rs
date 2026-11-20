//! The window's size and font size at start, from the command line

use smart_default::SmartDefault;

#[derive(Debug, Clone, SmartDefault)]
pub struct WindowConfig {
	#[default(String::from("Sarthi"))]
	pub title: String,
	#[default(1280)]
	pub width: u32,
	#[default(720)]
	pub height: u32,
	#[default(true)]
	pub resizable: bool,

	/// Font size, in logical pixels, unless `guifont` sets one
	#[default(14.0)]
	pub font_size: f32,
}
