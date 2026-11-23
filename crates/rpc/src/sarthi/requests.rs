//! Sarthi's API: what Lua asks of Sarthi, decoded straight from Neovim's requests. The method is the variant's name in
//! snake case, e.g. `rpcrequest(channel, "nvim_open_win", buf, enter, config)`, and the arguments are decoded in order
//! into the variant's struct. `runtime/lua/sarthi/` only wraps these calls.
//!
//! What the help file (`:help sarthi`) says about the Lua module and the window functions:
//!
#![doc = include_str!("doc/lua.md")]
//!
#![doc = include_str!("doc/api.md")]

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::sarthi::notifications::window_number;
use crate::{Buffer, DeserializeTaggedEnum, Register, Settings, Window, WindowConfig};

/// What Lua asks of Sarthi: a variant each, named like its Rust method in snake case. Those with `x-lua` are the Lua
/// module's functions, which call them.
#[derive(Debug, DeserializeTaggedEnum, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
#[schemars(rename_all = "snake_case")]
pub enum Request {
	/// Sets Sarthi's settings: those given, and the defaults of the rest, in place of what an earlier call gave.
	/// Applies at once, so it can be called again later, e.g. from a keymap. A mistake is an error at the call.
	#[schemars(extend("x-lua" = "setup"))]
	Setup(SetupArgs),

	/// The settings in effect: those given to `setup()`, and the defaults of the rest, e.g. to place a window past
	/// another's padding when it has the user's. A copy: changing it changes nothing.
	#[schemars(extend("x-lua" = "settings", "x-returns" = "sarthi.Settings"))]
	GetSettings(GetSettingsArgs),

	/// Sets the font size, in points (4 to 96), by setting `:h` in `guifont`. Window fonts with a size of their own keep
	/// it.
	#[schemars(extend("x-lua" = "set_font_size"))]
	SetFontSize(SetFontSizeArgs),

	/// Makes the font `delta` points bigger, or smaller if negative, by setting `:h` in `guifont`, and each window font
	/// with a size of its own (`sarthi.font.size`) too. Sizes stay within 4 to 96 points.
	#[schemars(extend("x-lua" = "change_font_size"))]
	ChangeFontSize(ChangeFontSizeArgs),

	/// Like `vim.api.nvim_open_win()`, with a `sarthi` table in `config` ([`WindowConfig`](#sarthi.WindowConfig)).
	/// Managed even without one: it has the settings' outline, corners and padding. A mistake in the `sarthi` table
	/// closes the window again, so the call does nothing but fail.
	#[schemars(extend("x-lua" = "api.nvim_open_win", "x-returns" = "integer"))]
	NvimOpenWin(NvimOpenWinArgs),

	/// Like `vim.api.nvim_win_set_config()`, with a `sarthi` table in `config`: what it gives replaces what was given
	/// before, key by key, and a section's values key by key too.
	#[schemars(extend("x-lua" = "api.nvim_win_set_config"))]
	NvimWinSetConfig(NvimWinSetConfigArgs),

	/// Like `vim.api.nvim_win_get_config()`, with the window's `sarthi` table: its `w:sarthi`, and what else was given.
	#[schemars(extend("x-lua" = "api.nvim_win_get_config", "x-returns" = "vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }"))]
	NvimWinGetConfig(NvimWinGetConfigArgs),

	/// The clipboard provider copying to `+` or `*`
	#[schemars(skip)]
	SetClipboard(SetClipboardArgs),

	/// The clipboard provider pasting from `+` or `*`
	#[schemars(skip)]
	GetClipboard(GetClipboardArgs),

	/// A method sarthi doesn't know
	#[schemars(skip)]
	Other {
		/// Its name
		method: String,

		/// Its arguments, skipped
		value: IgnoredAny,
	},
}

/// The arguments of [`Request::Setup`]
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetupArgs {
	/// The settings ([Settings](#sarthi-settings)); left out, all the defaults
	// Boxed: the settings are large, and other messages shouldn't take their room.
	pub settings: Option<Box<Settings>>,
}

/// The arguments of [`Request::GetSettings`]: none
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GetSettingsArgs {}

/// The arguments of [`Request::SetFontSize`]
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetFontSizeArgs {
	/// Size in points
	pub points: f32,
}

/// The arguments of [`Request::ChangeFontSize`]
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ChangeFontSizeArgs {
	/// Points to add, or to take away if negative
	pub delta: f32,
}

/// The arguments of [`Request::NvimOpenWin`]
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
pub struct NvimOpenWinArgs {
	/// The buffer it shows
	pub buf: Buffer,

	/// Whether it becomes the current window
	pub enter: bool,

	/// Neovim's window config, with a `sarthi` table in it
	#[schemars(with = "NvimWinConfig")]
	pub config: rmpv::Value,
}

/// The arguments of [`Request::NvimWinSetConfig`]
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
pub struct NvimWinSetConfigArgs {
	/// The window, or 0 for the current one
	#[serde(deserialize_with = "window_number")]
	pub win: Window,

	/// Neovim's window config, with a `sarthi` table in it
	#[schemars(with = "NvimWinConfig")]
	pub config: rmpv::Value,
}

/// The arguments of [`Request::NvimWinGetConfig`]
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
pub struct NvimWinGetConfigArgs {
	/// The window, or 0 for the current one
	#[serde(deserialize_with = "window_number")]
	pub win: Window,
}

/// The clipboard provider's copy
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetClipboardArgs {
	/// `+` or `*`
	pub register: Register,
	/// The lines copied
	pub lines: Vec<String>,

	/// `v`, `V` or a block like `^V10`
	pub regtype: String,
}

/// The clipboard provider's paste
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GetClipboardArgs {
	/// `+` or `*`
	pub register: Register,
}

/// The schema of the window functions' `config`: Neovim's window config (`:help nvim_open_win()`), with a `sarthi`
/// table in it ([`WindowConfig`]). Decoded as a value, since Neovim's part goes on to Neovim as given.
struct NvimWinConfig;

impl JsonSchema for NvimWinConfig {
	/// Written where it's used, so its Lua type goes with it
	fn inline_schema() -> bool {
		true
	}

	fn schema_name() -> std::borrow::Cow<'static, str> {
		"NvimWinConfig".into()
	}

	fn json_schema(generator: &mut SchemaGenerator) -> Schema {
		json_schema!({
			"type": "object",
			"description": "Neovim's window config (`:help nvim_open_win()`), with a `sarthi` table in it",
			"properties": { "sarthi": generator.subschema_for::<WindowConfig>() },
			"x-lua-type": "vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }",
		})
	}
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
