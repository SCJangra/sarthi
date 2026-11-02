use arrayvec::ArrayString;
use serde::Deserialize;

/// Text of one grid cell: a single character or a grapheme cluster.
///
/// Neovim caps a cell at 31 bytes of UTF-8 (its cell buffer is 32 bytes including a NUL), so the
/// text always fits inline. It is empty for the right half of a double-width character.
pub type CellText = ArrayString<32>;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GridLineEvent {
	pub grid: u32,
	pub row: u32,
	pub col_start: u32,
	pub cells: Vec<GridCell>,
	pub wrap: bool,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GridCell {
	pub text: CellText,
	#[serde(default)]
	pub hl_id: Option<u32>,
	#[serde(default)]
	pub repeat: Option<u32>,
}

#[cfg(test)]
mod tests {
	use super::{CellText, GridCell, GridLineEvent};
	use crate::rpc::notification::redraw::RedrawNotification;

	#[test]
	fn de_grid_line() {
		let expected = RedrawNotification::GridLine(vec![GridLineEvent {
			grid: 1,
			row: 0,
			col_start: 0,
			cells: vec![
				GridCell { text: CellText::from("a").unwrap(), hl_id: Some(1), repeat: Some(2) },
				GridCell { text: CellText::from("b").unwrap(), hl_id: None, repeat: None },
				GridCell { text: CellText::from("c").unwrap(), hl_id: Some(3), repeat: None },
			],
			wrap: true,
		}]);

		let event = vec![
			0x92, 0xA9, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x6C, 0x69, 0x6E, 0x65, 0x95, 0x01, 0x00,
			0x00, 0x93, 0x93, 0xA1, 0x61, 0x01, 0x02, 0x91, 0xA1, 0x62, 0x92, 0xA1, 0x63, 0x03,
			0xC3,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}

	#[test]
	fn de_grid_line_wide_and_combining() {
		let expected = RedrawNotification::GridLine(vec![GridLineEvent {
			grid: 1,
			row: 0,
			col_start: 0,
			cells: vec![
				GridCell { text: CellText::from("中").unwrap(), hl_id: Some(1), repeat: None },
				GridCell { text: CellText::new(), hl_id: None, repeat: None },
				GridCell { text: CellText::from("e\u{301}").unwrap(), hl_id: None, repeat: None },
			],
			wrap: false,
		}]);

		// ["grid_line", [1, 0, 0, [["中", 1], [""], ["e\u{301}"]], false]]
		let event = vec![
			0x92, 0xA9, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x6C, 0x69, 0x6E, 0x65, 0x95, 0x01, 0x00,
			0x00, 0x93, 0x92, 0xA3, 0xE4, 0xB8, 0xAD, 0x01, 0x91, 0xA0, 0x91, 0xA3, 0x65, 0xCC,
			0x81, 0xC2,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
