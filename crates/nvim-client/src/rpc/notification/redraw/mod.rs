mod default_colors_set;
mod grid_clear;
mod grid_cursor_goto;
mod grid_line;
mod grid_resize;
mod grid_scroll;
mod hl_attr_define;
mod hl_group_set;
mod mode_change;
mod mode_info_set;
mod win_pos;
mod win_viewport;

use nvui_derive::DeserializeTaggedEnum;

pub use default_colors_set::*;
pub use grid_clear::*;
pub use grid_cursor_goto::*;
pub use grid_line::*;
pub use grid_resize::*;
pub use grid_scroll::*;
pub use hl_attr_define::*;
pub use hl_group_set::*;
pub use mode_change::*;
pub use mode_info_set::*;
pub use win_pos::*;
pub use win_viewport::*;

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum RedrawNotification {
	GridResize(#[tagged_enum(flatten)] Vec<GridResizeEvent>),

	GridClear(#[tagged_enum(flatten)] Vec<GridClearEvent>),

	GridCursorGoto(#[tagged_enum(flatten)] Vec<GridCursorGotoEvent>),

	GridLine(#[tagged_enum(flatten)] Vec<GridLineEvent>),

	GridScroll(#[tagged_enum(flatten)] Vec<GridScrollEvent>),

	GridDestroy(#[tagged_enum(flatten)] Vec<GridDestroyEvent>),

	WinPos(#[tagged_enum(flatten)] Vec<WinPosEvent>),

	WinFloatPos(#[tagged_enum(flatten)] Vec<WinFloatPosEvent>),

	WinHide(#[tagged_enum(flatten)] Vec<WinHideEvent>),

	WinClose(#[tagged_enum(flatten)] Vec<WinCloseEvent>),

	WinViewport(#[tagged_enum(flatten)] Vec<WinViewportEvent>),

	WinViewportMargins(#[tagged_enum(flatten)] Vec<WinViewportMarginsEvent>),

	MsgSetPos(#[tagged_enum(flatten)] Vec<MsgSetPosEvent>),

	DefaultColorsSet(#[tagged_enum(flatten)] Vec<DefaultColorsSetEvent>),

	HlAttrDefine(#[tagged_enum(flatten)] Vec<HlAttrDefineEvent>),

	HlGroupSet(#[tagged_enum(flatten)] Vec<HlGroupSetEvent>),

	ModeInfoSet(#[tagged_enum(flatten)] Vec<ModeInfoSetEvent>),

	ModeChange(#[tagged_enum(flatten)] Vec<ModeChangeEvent>),

	/// The UI must stop drawing the cursor until `BusyStop` (`:help ui-global`)
	BusyStart,

	BusyStop,

	/// Neovim is done redrawing the screen, so this is the time to show the result (`:help ui-global`)
	Flush,

	/// An event nvui doesn't handle. Its arguments are skipped without being decoded; `value` only keeps
	/// one placeholder per occurrence of the event.
	Other {
		method: String,
		#[tagged_enum(flatten)]
		value: Vec<serde::de::IgnoredAny>,
	},
}

#[cfg(test)]
mod tests {
	use super::{GridCursorGotoEvent, RedrawNotification};

	#[test]
	fn de_other() {
		let expected = RedrawNotification::Other {
			method: String::from("unknown"),
			value: vec![serde::de::IgnoredAny, serde::de::IgnoredAny],
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
	fn de_flush() {
		// ["flush", []]: Neovim sends one empty argument list for events without parameters.
		let event = vec![0x92, 0xA5, 0x66, 0x6C, 0x75, 0x73, 0x68, 0x90];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, RedrawNotification::Flush)
	}

	#[test]
	fn de_busy_start_and_stop() {
		// ["busy_start", []] and ["busy_stop", []]
		let start = [&[0x92, 0xAA][..], b"busy_start", &[0x90]].concat();
		let stop = [&[0x92, 0xA9][..], b"busy_stop", &[0x90]].concat();

		assert_eq!(rmp_serde::from_slice::<RedrawNotification>(&start).unwrap(), RedrawNotification::BusyStart);
		assert_eq!(rmp_serde::from_slice::<RedrawNotification>(&stop).unwrap(), RedrawNotification::BusyStop);
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
