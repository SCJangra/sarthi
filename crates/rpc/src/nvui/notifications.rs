//! What nvui's Lua tells nvui without waiting for an answer, so Neovim goes on at once. Each is a variant of
//! [`Notification`](crate::Notification), with the method named like a request's.

use serde::{Deserialize, Deserializer};

use crate::Window;

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

	/// The window's `w:nvui_smooth_scroll`, if set
	#[serde(default)]
	pub smooth_scroll: Option<bool>,
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

	/// The window's `w:nvui_shadow`, if set
	#[serde(default)]
	pub shadow: Option<bool>,
}

/// Lua's window handles are plain numbers.
fn window_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Window, D::Error> {
	i64::deserialize(deserializer).map(Window)
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{FloatWindow, FloatsChangedArgs, ScrolledWindow, WinScrolledArgs};
	use crate::{Notification, Window};

	#[test]
	fn win_scrolled() {
		// `rpcnotify(channel, 'win_scrolled', { { win = 1000, filetype = 'lua' }, … })`
		let window = |win: i64, filetype: &str, smooth: Option<bool>| {
			let mut fields =
				vec![(Value::from("win"), Value::from(win)), (Value::from("filetype"), Value::from(filetype))];
			fields.extend(smooth.map(|smooth| (Value::from("smooth_scroll"), Value::from(smooth))));
			Value::Map(fields)
		};
		let windows = Value::Array(vec![window(1000, "lua", None), window(1001, "", Some(false))]);
		let notification = Value::Array(vec![Value::from("win_scrolled"), Value::Array(vec![windows])]);
		let decoded: Notification = rmp_serde::from_slice(&rmp_serde::to_vec(&notification).unwrap()).unwrap();

		let expected = WinScrolledArgs {
			windows: vec![
				ScrolledWindow { win: Window(1000), filetype: String::from("lua"), smooth_scroll: None },
				ScrolledWindow { win: Window(1001), filetype: String::new(), smooth_scroll: Some(false) },
			],
		};
		assert_eq!(decoded, Notification::WinScrolled(expected));
	}

	#[test]
	fn floats_changed() {
		// `rpcnotify(channel, 'floats_changed', { { win = 1002, filetype = '', shadow = false } })`
		let fields = vec![
			(Value::from("win"), Value::from(1002)),
			(Value::from("filetype"), Value::from("")),
			(Value::from("shadow"), Value::from(false)),
		];
		let windows = Value::Array(vec![Value::Map(fields)]);
		let notification = Value::Array(vec![Value::from("floats_changed"), Value::Array(vec![windows])]);
		let decoded: Notification = rmp_serde::from_slice(&rmp_serde::to_vec(&notification).unwrap()).unwrap();

		let window = FloatWindow { win: Window(1002), filetype: String::new(), shadow: Some(false) };
		assert_eq!(decoded, Notification::FloatsChanged(FloatsChangedArgs { windows: vec![window] }));
	}
}
