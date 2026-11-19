//! Sarthi's API: what Lua asks of Sarthi, decoded straight from Neovim's requests. The method is the variant's name in
//! snake case, e.g. `rpcrequest(channel, "nvim_open_win", buf, enter, config)`, and the arguments are decoded in order
//! into the variant's struct. `runtime/lua/sarthi/` only wraps these calls.

#![allow(missing_docs, reason = "the arguments of sarthi's Lua API, which `runtime/lua/sarthi/api.lua` documents")]

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::sarthi::notifications::window_number;
use crate::{Buffer, DeserializeTaggedEnum, Register, Settings, Window};

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Request {
	/// `require("sarthi").setup(opts)`, with the settings given
	Setup(SetupArgs),

	/// `require("sarthi").settings()`; answered with the settings in effect: those given to `setup()`, and the defaults
	/// of the rest
	GetSettings(GetSettingsArgs),

	/// `require("sarthi").set_font_size(points)`, which sets `guifont`
	SetFontSize(SetFontSizeArgs),

	/// `require("sarthi").change_font_size(delta)`, which sets `guifont`
	ChangeFontSize(ChangeFontSizeArgs),

	/// `require("sarthi").reset_font_size()`, which sets `guifont`
	ResetFontSize(ResetFontSizeArgs),

	/// `require("sarthi").api.nvim_open_win(buf, enter, config)`; answered with the window
	NvimOpenWin(NvimOpenWinArgs),

	/// `require("sarthi").api.nvim_win_set_config(win, config)`
	NvimWinSetConfig(NvimWinSetConfigArgs),

	/// `require("sarthi").api.nvim_win_get_config(win)`; answered with the window's config
	NvimWinGetConfig(NvimWinGetConfigArgs),

	/// The clipboard provider copying to `+` or `*`
	SetClipboard(SetClipboardArgs),

	/// The clipboard provider pasting from `+` or `*`
	GetClipboard(GetClipboardArgs),

	/// A method sarthi doesn't know
	Other { method: String, value: IgnoredAny },
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetupArgs {
	/// The settings given, or none for all the defaults. Boxed: the settings are large, and other messages shouldn't
	/// take their room.
	pub settings: Option<Box<Settings>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GetSettingsArgs {}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetFontSizeArgs {
	pub points: f32,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ChangeFontSizeArgs {
	/// In points
	pub delta: f32,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ResetFontSizeArgs {}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct NvimOpenWinArgs {
	pub buf: Buffer,
	pub enter: bool,

	/// Neovim's window config, with a `sarthi` table in it
	pub config: rmpv::Value,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct NvimWinSetConfigArgs {
	/// The window, or 0 for the current one
	#[serde(deserialize_with = "window_number")]
	pub win: Window,

	/// Neovim's window config, with a `sarthi` table in it
	pub config: rmpv::Value,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct NvimWinGetConfigArgs {
	/// The window, or 0 for the current one
	#[serde(deserialize_with = "window_number")]
	pub win: Window,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetClipboardArgs {
	pub register: Register,
	pub lines: Vec<String>,

	/// `v`, `V` or a block like `^V10`
	pub regtype: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GetClipboardArgs {
	pub register: Register,
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{Request, SetClipboardArgs, SetFontSizeArgs};
	use crate::Register;

	/// `[method, [args…]]`, the way Neovim sends a request after its id
	fn decode(method: &str, args: Vec<Value>) -> Result<Request, String> {
		let request = Value::Array(vec![Value::from(method), Value::Array(args)]);
		rmpv::ext::from_value(request).map_err(|rmpv::ext::Error::Syntax(message)| message)
	}

	#[test]
	fn arguments_decode_in_order() {
		let lines = Value::Array(vec![Value::from("one"), Value::from("")]);
		let request = decode("set_clipboard", vec![Value::from("+"), lines, Value::from("V")]).unwrap();
		let Request::SetClipboard(SetClipboardArgs { register, lines, regtype }) = request else {
			panic!("{request:?}")
		};
		assert_eq!(
			(register, lines, regtype.as_str()),
			(Register::Clipboard, vec![String::from("one"), String::new()], "V")
		);

		let request = decode("get_clipboard", vec![Value::from("*")]).unwrap();
		assert!(matches!(&request, Request::GetClipboard(args) if args.register == Register::Primary), "{request:?}");

		// An empty Lua table arrives as an array: all settings at their defaults
		let request = decode("setup", vec![Value::Array(Vec::new())]).unwrap();
		assert!(matches!(request, Request::Setup(_)), "{request:?}");

		// No settings: the defaults
		let request = decode("setup", vec![Value::Nil]).unwrap();
		assert_eq!(request, Request::Setup(super::SetupArgs { settings: None }));

		// A Lua integer as a size
		let request = decode("set_font_size", vec![Value::from(14)]).unwrap();
		let expected = SetFontSizeArgs { points: 14.0 };
		assert_eq!(request, Request::SetFontSize(expected));
	}

	#[test]
	fn mismatches() {
		let error = decode("get_clipboard", vec![Value::from("a")]).unwrap_err();
		assert_eq!(error, "no clipboard register `a`, expected `+` or `*`");

		let settings = Value::Map(vec![(Value::from("scrol"), Value::Array(Vec::new()))]);
		let error = decode("setup", vec![settings]).unwrap_err();
		assert!(error.contains("unknown field `scrol`"), "{error}");

		assert!(matches!(decode("get_settings", vec![]), Ok(Request::GetSettings(_))));

		let request = decode("unknown", vec![]).unwrap();
		assert!(matches!(&request, Request::Other { method, .. } if method == "unknown"), "{request:?}");
	}
}
