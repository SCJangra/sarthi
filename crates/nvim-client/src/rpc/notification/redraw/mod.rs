mod default_colors_set;
mod grid_clear;
mod grid_cursor_goto;
mod grid_line;
mod grid_resize;
mod hl_attr_define;
mod hl_group_set;
mod mode_info_set;

use nvui_derive::DeserializeTaggedEnum;

pub use default_colors_set::*;
pub use grid_clear::*;
pub use grid_cursor_goto::*;
pub use grid_line::*;
pub use grid_resize::*;
pub use hl_attr_define::*;
pub use hl_group_set::*;
pub use mode_info_set::*;

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum RedrawNotification {
	GridResize(#[tagged_enum(flatten)] Vec<GridResizeEvent>),

	GridClear(#[tagged_enum(flatten)] Vec<GridClearEvent>),

	GridCursorGoto(#[tagged_enum(flatten)] Vec<GridCursorGotoEvent>),

	GridLine(#[tagged_enum(flatten)] Vec<GridLineEvent>),

	DefaultColorsSet(#[tagged_enum(flatten)] Vec<DefaultColorsSetEvent>),

	HlAttrDefine(#[tagged_enum(flatten)] Vec<HlAttrDefineEvent>),

	HlGroupSet(#[tagged_enum(flatten)] Vec<HlGroupSetEvent>),

	ModeInfoSet(#[tagged_enum(flatten)] Vec<ModeInfoSetEvent>),

	Other {
		method: String,
		#[tagged_enum(flatten)]
		value: Vec<rmpv::Value>,
	},
}

#[cfg(test)]
mod tests {
	use super::{GridCursorGotoEvent, RedrawNotification};

	#[test]
	fn de_other() {
		let expected = RedrawNotification::Other {
			method: String::from("unknown"),
			value: vec![
				rmpv::Value::from("simple_string"),
				rmpv::Value::from(vec![
					rmpv::Value::from("array_string_1"),
					rmpv::Value::from("array_string_2"),
				]),
			],
		};

		let event = vec![
			0x93, 0xA7, 0x75, 0x6E, 0x6B, 0x6E, 0x6F, 0x77, 0x6E, 0xAD, 0x73, 0x69, 0x6D, 0x70,
			0x6C, 0x65, 0x5F, 0x73, 0x74, 0x72, 0x69, 0x6E, 0x67, 0x92, 0xAE, 0x61, 0x72, 0x72,
			0x61, 0x79, 0x5F, 0x73, 0x74, 0x72, 0x69, 0x6E, 0x67, 0x5F, 0x31, 0xAE, 0x61, 0x72,
			0x72, 0x61, 0x79, 0x5F, 0x73, 0x74, 0x72, 0x69, 0x6E, 0x67, 0x5F, 0x32,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}

	#[test]
	fn de_ignores_trailing_arguments() {
		let expected = RedrawNotification::GridCursorGoto(vec![
			GridCursorGotoEvent { grid: 1, row: 5, col: 9 },
			GridCursorGotoEvent { grid: 2, row: 7, col: 3 },
		]);

		// ["grid_cursor_goto", [1, 5, 9, "new"], [2, 7, 3]]: a newer Neovim may append parameters
		let event = vec![
			0x93, 0xB0, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x63, 0x75, 0x72, 0x73, 0x6F, 0x72, 0x5F,
			0x67, 0x6F, 0x74, 0x6F, 0x94, 0x01, 0x05, 0x09, 0xA3, 0x6E, 0x65, 0x77, 0x93, 0x02,
			0x07, 0x03,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
