//! What sarthi's Lua (`runtime/`) asks of sarthi, decoded straight from Neovim's requests. The method is the variant's
//! name in snake case, e.g. `rpcrequest(channel, "set_clipboard", register, lines, regtype)`, and the arguments are
//! decoded in order into the variant's struct.

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::sarthi::notifications::window_number;
use crate::{DeserializeTaggedEnum, Register, Settings, Window, WindowFont, WindowOptions, WindowPlace};

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Request {
	/// `require("sarthi").setup(opts)`, with the settings given so far merged
	Setup(SetupArgs),

	/// The clipboard provider copying to `+` or `*`
	SetClipboard(SetClipboardArgs),

	/// The clipboard provider pasting from `+` or `*`
	GetClipboard(GetClipboardArgs),

	/// `require("sarthi").set_font_size(points)`; answered with the `guifont` to set
	SetFontSize(SetFontSizeArgs),

	/// `require("sarthi").change_font_size(delta)`; answered with the `guifont` to set
	ChangeFontSize(ChangeFontSizeArgs),

	/// `require("sarthi").reset_font_size()`; answered with the `guifont` to set
	ResetFontSize(ResetFontSizeArgs),

	/// `require("sarthi").api` placing external window `win` (`sarthi.place`), or putting it back in the middle for no place;
	/// answered with what's wrong with the place, if anything
	PlaceWindow(PlaceWindowArgs),

	/// `require("sarthi").api` checking a window's own values of the settings (`w:sarthi`) before setting them; answered
	/// with what's wrong with their values, if anything. One sarthi can't decode fails the request.
	CheckWindowOptions(CheckWindowOptionsArgs),
	/// `require("sarthi").api` giving external window `win` its own fonts (`sarthi.font`), or the main ones for none;
	/// answered with what's wrong with them, if anything
	WindowFont(WindowFontArgs),

	/// A method sarthi doesn't know
	Other { method: String, value: IgnoredAny },
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetupArgs {
	/// Boxed: the settings are large, and other messages shouldn't take their room.
	pub settings: Box<Settings>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CheckWindowOptionsArgs {
	/// Boxed like the settings
	pub options: Box<WindowOptions>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct PlaceWindowArgs {
	#[serde(deserialize_with = "window_number")]
	pub win: Window,

	/// Boxed like the settings, so other messages don't take its room
	pub place: Option<Box<WindowPlace>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WindowFontArgs {
	#[serde(deserialize_with = "window_number")]
	pub win: Window,

	pub font: Option<Box<WindowFont>>,
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

/// The font size functions send `guifont` as it is when called, so changes in a row each start from the last one.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetFontSizeArgs {
	pub guifont: String,
	pub points: f32,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ChangeFontSizeArgs {
	pub guifont: String,

	/// In points
	pub delta: f32,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ResetFontSizeArgs {
	pub guifont: String,
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

		// A Lua integer as a size
		let request = decode("set_font_size", vec![Value::from("X"), Value::from(14)]).unwrap();
		let expected = SetFontSizeArgs { guifont: String::from("X"), points: 14.0 };
		assert_eq!(request, Request::SetFontSize(expected));
	}

	#[test]
	fn mismatches() {
		let error = decode("get_clipboard", vec![Value::from("a")]).unwrap_err();
		assert_eq!(error, "no clipboard register `a`, expected `+` or `*`");

		let settings = Value::Map(vec![(Value::from("scrol"), Value::Array(Vec::new()))]);
		let error = decode("setup", vec![settings]).unwrap_err();
		assert!(error.contains("unknown field `scrol`"), "{error}");

		// A window's options: only what the settings have
		let options =
			Value::Map(vec![(Value::from("shadow"), Value::Map(vec![(Value::from("blur"), Value::from(4))]))]);
		assert!(matches!(decode("check_window_options", vec![options]), Ok(Request::CheckWindowOptions(_))));
		let options = Value::Map(vec![(Value::from("shadow"), Value::from(true))]);
		let error = decode("check_window_options", vec![options]).unwrap_err();
		assert!(error.contains("invalid type"), "{error}");

		let request = decode("unknown", vec![]).unwrap();
		assert!(matches!(&request, Request::Other { method, .. } if method == "unknown"), "{request:?}");
	}
}
