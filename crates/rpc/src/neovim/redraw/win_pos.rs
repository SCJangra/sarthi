use serde::Deserialize;
use serde::de::IgnoredAny;

/// Shows `grid` as a window at `start_row`, `start_col` of grid 1, `width` × `height` cells (`:help ui-multigrid`).
/// Also shows it again after `win_hide`.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WinPosEvent {
	pub grid: u32,

	/// The window handle, unused
	pub win: IgnoredAny,

	pub start_row: u32,
	pub start_col: u32,
	pub width: u32,
	pub height: u32,
}

/// Shows `grid` as a floating window. Neovim places it at `screen_row`, `screen_col` and orders the floats by
/// `compindex`; the anchor describes how it's positioned relative to `anchor_grid`, for UIs placing it themselves.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WinFloatPosEvent {
	pub grid: u32,

	/// The window handle, unused
	pub win: IgnoredAny,

	/// "NW", "NE", "SW" or "SE"
	pub anchor: String,
	pub anchor_grid: i64,
	pub anchor_row: f64,
	pub anchor_col: f64,

	pub mouse_enabled: bool,

	// Sent since Neovim 0.10
	#[serde(default)]
	pub zindex: u32,
	#[serde(default)]
	pub compindex: u32,
	#[serde(default)]
	pub screen_row: u32,
	#[serde(default)]
	pub screen_col: u32,
}

/// Hides the window of `grid`, e.g. when switching tabs; it can be shown again later
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WinHideEvent {
	pub grid: u32,
}

/// Closes the window of `grid`
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WinCloseEvent {
	pub grid: u32,
}

/// Frees `grid`; it won't be used again
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GridDestroyEvent {
	pub grid: u32,
}

/// Shows the message grid at `row` of grid 1, across its full width (`:help ui-multigrid`)
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct MsgSetPosEvent {
	pub grid: u32,
	pub row: u32,

	/// Whether messages scrolled up over other windows; the TUI then draws a line of `sep_char` above them
	pub scrolled: bool,
	pub sep_char: String,

	// Sent since Neovim 0.10
	#[serde(default)]
	pub zindex: u32,
	#[serde(default)]
	pub compindex: u32,
}

#[cfg(test)]
mod tests {
	use rmpv::Value;
	use serde::de::IgnoredAny;

	use super::{MsgSetPosEvent, WinCloseEvent, WinFloatPosEvent, WinHideEvent, WinPosEvent};
	use crate::RedrawNotification;

	/// Decodes `[name, args...]` the way Neovim sends it
	fn decode(name: &str, args: Vec<Vec<Value>>) -> RedrawNotification {
		let event: Vec<Value> = std::iter::once(Value::from(name))
			.chain(args.into_iter().map(Value::Array))
			.collect();
		rmp_serde::from_slice(&rmp_serde::to_vec(&Value::Array(event)).unwrap()).unwrap()
	}

	/// A window handle: msgpack extension type 1
	fn win(id: u8) -> Value {
		Value::Ext(1, vec![0xCD, 0x03, id])
	}

	#[test]
	fn de_win_pos() {
		// From Neovim 0.12: ["win_pos", [2, win, 0, 31, 29, 13]]
		let event = decode("win_pos", vec![vec![2.into(), win(0xE8), 0.into(), 31.into(), 29.into(), 13.into()]]);
		let expected = WinPosEvent { grid: 2, win: IgnoredAny, start_row: 0, start_col: 31, width: 29, height: 13 };
		assert_eq!(event, RedrawNotification::WinPos(vec![expected]));
	}

	#[test]
	fn de_win_float_pos() {
		// From Neovim 0.12: a float at (3, 10) with compindex 1, and the popup menu anchored to grid 5
		let float = vec![
			6.into(),
			win(0xEB),
			"NW".into(),
			1.into(),
			3.into(),
			10.into(),
			true.into(),
			50.into(),
			1.into(),
			3.into(),
			10.into(),
		];
		let event = decode("win_float_pos", vec![float]);
		let expected = WinFloatPosEvent {
			grid: 6,
			win: IgnoredAny,
			anchor: String::from("NW"),
			anchor_grid: 1,
			anchor_row: 3.0,
			anchor_col: 10.0,
			mouse_enabled: true,
			zindex: 50,
			compindex: 1,
			screen_row: 3,
			screen_col: 10,
		};
		assert_eq!(event, RedrawNotification::WinFloatPos(vec![expected]));

		// Before Neovim 0.10 the last four were missing.
		let old = decode(
			"win_float_pos",
			vec![vec![6.into(), win(1), "SE".into(), 1.into(), 2.5.into(), 0.into(), false.into()]],
		);
		let RedrawNotification::WinFloatPos(events) = old else { panic!("{old:?}") };
		assert_eq!((events[0].anchor_row, events[0].compindex, events[0].screen_row), (2.5, 0, 0));
	}

	#[test]
	fn de_win_hide_close_and_grid_destroy() {
		assert_eq!(
			decode("win_hide", vec![vec![5.into()]]),
			RedrawNotification::WinHide(vec![WinHideEvent { grid: 5 }])
		);
		assert_eq!(
			decode("win_close", vec![vec![7.into()]]),
			RedrawNotification::WinClose(vec![WinCloseEvent { grid: 7 }])
		);
		assert_eq!(
			decode("grid_destroy", vec![vec![7.into()], vec![8.into()]]),
			RedrawNotification::GridDestroy(vec![
				super::GridDestroyEvent { grid: 7 },
				super::GridDestroyEvent { grid: 8 }
			])
		);
	}

	#[test]
	fn de_msg_set_pos() {
		// From Neovim 0.12, after a long message scrolled up
		let event =
			decode("msg_set_pos", vec![vec![3.into(), 10.into(), true.into(), " ".into(), 200.into(), 2.into()]]);
		let expected =
			MsgSetPosEvent { grid: 3, row: 10, scrolled: true, sep_char: String::from(" "), zindex: 200, compindex: 2 };
		assert_eq!(event, RedrawNotification::MsgSetPos(vec![expected]));
	}
}
