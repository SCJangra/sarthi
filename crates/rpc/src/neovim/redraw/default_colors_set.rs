use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct DefaultColorsSetEvent {
	/// Default foreground color (RGB).
	pub rgb_fg: u32,

	/// Default background color (RGB).
	pub rgb_bg: u32,

	/// Default special color (RGB).
	pub rgb_sp: u32,

	/// Default terminal foreground color.
	pub cterm_fg: i32,

	/// Default terminal background color.
	pub cterm_bg: i32,
}

#[cfg(test)]
mod tests {
	use super::DefaultColorsSetEvent;
	use crate::RedrawNotification;

	#[test]
	fn de_default_colors_set() {
		let expected = RedrawNotification::DefaultColorsSet(vec![DefaultColorsSetEvent {
			rgb_fg: 0x112233,
			rgb_bg: 0x445566,
			rgb_sp: 0x778899,
			cterm_fg: 1,
			cterm_bg: -1,
		}]);

		let event = vec![
			0x92, 0xB2, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6C, 0x74, 0x5F, 0x63, 0x6F, 0x6C, 0x6F, 0x72, 0x73, 0x5F, 0x73,
			0x65, 0x74, 0x95, 0xCE, 0x00, 0x11, 0x22, 0x33, 0xCE, 0x00, 0x44, 0x55, 0x66, 0xCE, 0x00, 0x77, 0x88, 0x99,
			0x01, 0xFF,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
