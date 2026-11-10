use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GridCursorGotoEvent {
	pub grid: u32,
	pub row: u32,
	pub col: u32,
}

#[cfg(test)]
mod tests {
	use super::GridCursorGotoEvent;
	use crate::RedrawNotification;

	#[test]
	fn de_grid_cursor_goto() {
		let expected = RedrawNotification::GridCursorGoto(vec![
			GridCursorGotoEvent { grid: 1, row: 5, col: 9 },
			GridCursorGotoEvent { grid: 2, row: 7, col: 3 },
		]);

		let event = vec![
			0x93, 0xB0, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x63, 0x75, 0x72, 0x73, 0x6F, 0x72, 0x5F, 0x67, 0x6F, 0x74, 0x6F,
			0x93, 0x01, 0x05, 0x09, 0x93, 0x02, 0x07, 0x03,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
