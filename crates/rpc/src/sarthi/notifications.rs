//! What sarthi's Lua tells sarthi without waiting for an answer, so Neovim goes on at once. Each is a variant of
//! [`Notification`](crate::Notification), with the method named like a request's.

use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer};

use crate::{Window, WindowOptions};

/// Windows that scrolled (`WinScrolled`), sent before Neovim redraws them
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WinScrolledArgs {
	pub windows: Vec<ScrolledWindow>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ScrolledWindow {
	#[serde(deserialize_with = "window_number")]
	pub win: Window,

	/// Of the buffer the window shows
	pub filetype: String,

	/// The window's `w:sarthi`, if set
	#[serde(default)]
	pub sarthi: WindowVariable,
}

/// Floating windows whose filetype or variables changed since they were last sent, sent before Neovim draws them
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct FloatsChangedArgs {
	pub windows: Vec<FloatWindow>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct FloatWindow {
	#[serde(deserialize_with = "window_number")]
	pub win: Window,

	/// Of the buffer the window shows
	pub filetype: String,

	/// The window's `w:sarthi`, if set
	#[serde(default)]
	pub sarthi: WindowVariable,

	/// How transparent the window is, from 0 to 100 (`winblend`)
	#[serde(default)]
	pub winblend: u32,

	/// The text color of its outline's highlight group, as `0xRRGGBB`, if the group has one: Neovim only sends the
	/// colors of its UI groups
	#[serde(default)]
	pub outline_color: Option<u32>,

	/// The floating window it's attached to (`relative = 'win'`), like a scrollbar to its menu
	#[serde(default, deserialize_with = "optional_window_number")]
	pub parent: Option<Window>,
}

/// A window's `w:sarthi`, or a value that isn't one, e.g. with a key sarthi doesn't know: a mistake in one window's
/// table doesn't lose what's sent about the others.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum WindowVariable {
	Options(WindowOptions),
	Invalid(IgnoredAny),
}

impl Default for WindowVariable {
	/// Not set: nothing of the window's own
	fn default() -> Self {
		Self::Options(WindowOptions::default())
	}
}

/// Lua's window handles are plain numbers.
pub(crate) fn window_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Window, D::Error> {
	i64::deserialize(deserializer).map(Window)
}

pub(crate) fn optional_window_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Window>, D::Error> {
	Option::<i64>::deserialize(deserializer).map(|handle| handle.map(Window))
}

#[cfg(test)]
mod tests {
	use rmpv::Value;
	use serde::de::IgnoredAny;

	use super::{FloatWindow, FloatsChangedArgs, ScrolledWindow, WinScrolledArgs, WindowVariable};
	use crate::{Notification, ScrollOptions, ShadowOptions, Window, WindowOptions};

	fn decode(method: &str, windows: Vec<Value>) -> Notification {
		let notification = Value::Array(vec![Value::from(method), Value::Array(vec![Value::Array(windows)])]);
		rmp_serde::from_slice(&rmp_serde::to_vec(&notification).unwrap()).unwrap()
	}

	fn map(pairs: Vec<(&str, Value)>) -> Value {
		Value::Map(pairs.into_iter().map(|(key, value)| (Value::from(key), value)).collect())
	}

	#[test]
	fn win_scrolled() {
		// `rpcnotify(channel, 'win_scrolled', { { win = 1000, filetype = 'lua' }, … })`
		let scroll = map(vec![("scroll", map(vec![("enabled", Value::from(false))]))]);
		let windows = vec![
			map(vec![("win", Value::from(1000)), ("filetype", Value::from("lua"))]),
			map(vec![("win", Value::from(1001)), ("filetype", Value::from("")), ("sarthi", scroll)]),
		];

		let off = WindowOptions {
			scroll: ScrollOptions { enabled: Some(false), ..Default::default() },
			..Default::default()
		};
		let expected = WinScrolledArgs {
			windows: vec![
				ScrolledWindow { win: Window(1000), filetype: String::from("lua"), sarthi: WindowVariable::default() },
				ScrolledWindow { win: Window(1001), filetype: String::new(), sarthi: WindowVariable::Options(off) },
			],
		};
		assert_eq!(decode("win_scrolled", windows), Notification::WinScrolled(expected));
	}

	#[test]
	fn floats_changed() {
		// `rpcnotify(channel, 'floats_changed', { { win = 1002, filetype = '', sarthi = { shadow = { … } } } })`
		let shadow = map(vec![("enabled", Value::from(true)), ("blur", Value::from(4))]);
		let windows = vec![
			map(vec![
				("win", Value::from(1002)),
				("filetype", Value::from("")),
				("sarthi", map(vec![("shadow", shadow)])),
			]),
			map(vec![
				("win", Value::from(1003)),
				("filetype", Value::from("")),
				("winblend", Value::from(100)),
				("parent", Value::from(1002)),
			]),
			// A mistake in its `w:sarthi` doesn't lose the others.
			map(vec![
				("win", Value::from(1004)),
				("filetype", Value::from("")),
				("sarthi", map(vec![("shadow", Value::from(false))])),
			]),
		];

		let shadow = ShadowOptions { enabled: Some(true), blur: Some(4.0), ..Default::default() };
		let options = WindowOptions { shadow, ..Default::default() };
		let window = |win, sarthi| FloatWindow {
			win: Window(win),
			filetype: String::new(),
			sarthi,
			winblend: 0,
			outline_color: None,
			parent: None,
		};
		let expected = vec![
			window(1002, WindowVariable::Options(options)),
			FloatWindow { winblend: 100, parent: Some(Window(1002)), ..window(1003, WindowVariable::default()) },
			window(1004, WindowVariable::Invalid(IgnoredAny)),
		];
		assert_eq!(
			decode("floats_changed", windows),
			Notification::FloatsChanged(FloatsChangedArgs { windows: expected })
		);
	}
}
