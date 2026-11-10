use serde::Deserialize;
use serde::de::IgnoredAny;

/// Which buffer lines the window of `grid` shows, sent when they change (`:help ui-event-win_viewport`)
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WinViewportEvent {
	pub grid: u32,

	/// The window handle, unused
	pub win: IgnoredAny,

	/// First line shown, 0-based
	pub topline: u32,
	/// Line after the last one shown, 0-based
	pub botline: u32,
	pub curline: u32,
	pub curcol: u32,
	pub line_count: u32,

	/// How many lines the window scrolled since the last event: positive when the text moved up. Big jumps are
	/// approximate. Sent since Neovim 0.10.
	#[serde(default)]
	pub scroll_delta: i64,
}

/// Rows and columns at the edges of the window of `grid` that don't scroll with the text, like the winbar
/// (`:help ui-event-win_viewport_margins`)
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WinViewportMarginsEvent {
	pub grid: u32,

	/// The window handle, unused
	pub win: IgnoredAny,

	pub top: u32,
	pub bottom: u32,
	pub left: u32,
	pub right: u32,
}

#[cfg(test)]
mod tests {
	use rmpv::Value;
	use serde::de::IgnoredAny;

	use super::{WinViewportEvent, WinViewportMarginsEvent};
	use crate::RedrawNotification;

	/// Decodes `[name, args...]` the way Neovim sends it
	fn decode(name: &str, args: Vec<Value>) -> RedrawNotification {
		let event = Value::Array(vec![Value::from(name), Value::Array(args)]);
		rmp_serde::from_slice(&rmp_serde::to_vec(&event).unwrap()).unwrap()
	}

	/// A window handle: msgpack extension type 1
	fn win() -> Value {
		Value::Ext(1, vec![0xCD, 0x03, 0xE8])
	}

	#[test]
	fn de_win_viewport() {
		// From Neovim 0.12, after scrolling up a line
		let args = [2, 16, 3, 0, 500, -1].into_iter().map(Value::from);
		let event = decode("win_viewport", [2.into(), win()].into_iter().chain(args).collect());
		let expected = WinViewportEvent {
			grid: 2,
			win: IgnoredAny,
			topline: 2,
			botline: 16,
			curline: 3,
			curcol: 0,
			line_count: 500,
			scroll_delta: -1,
		};
		assert_eq!(event, RedrawNotification::WinViewport(vec![expected]));

		// Before Neovim 0.10 there was no scroll delta.
		let args = [0, 2, 0, 0, 1].into_iter().map(Value::from);
		let old = decode("win_viewport", [2.into(), win()].into_iter().chain(args).collect());
		let RedrawNotification::WinViewport(events) = old else { panic!("{old:?}") };
		assert_eq!(events[0].scroll_delta, 0);
	}

	#[test]
	fn de_win_viewport_margins() {
		// From Neovim 0.12, with a winbar
		let args = [1, 0, 0, 0].into_iter().map(Value::from);
		let event = decode("win_viewport_margins", [2.into(), win()].into_iter().chain(args).collect());
		let expected = WinViewportMarginsEvent { grid: 2, win: IgnoredAny, top: 1, bottom: 0, left: 0, right: 0 };
		assert_eq!(event, RedrawNotification::WinViewportMargins(vec![expected]));
	}
}
