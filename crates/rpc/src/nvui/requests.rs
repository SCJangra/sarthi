//! What nvui's Lua (`lua/*.lua`) asks of nvui, decoded straight from Neovim's requests. The method is the variant's name
//! in snake case, e.g. `rpcrequest(channel, "set_clipboard", register, lines, regtype)`, and the arguments are decoded
//! in order into the variant's struct.

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::{DeserializeTaggedEnum, Register, Settings};

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Request {
	/// `require("nvui").setup(opts)`, with the settings given so far merged
	Setup(SetupArgs),

	/// The clipboard provider copying to `+` or `*`
	SetClipboard(SetClipboardArgs),

	/// The clipboard provider pasting from `+` or `*`
	GetClipboard(GetClipboardArgs),

	/// A method nvui doesn't know
	Other { method: String, value: IgnoredAny },
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SetupArgs {
	pub settings: Settings,
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

	use super::{Request, SetClipboardArgs};
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
	}

	#[test]
	fn mismatches() {
		let error = decode("get_clipboard", vec![Value::from("a")]).unwrap_err();
		assert_eq!(error, "no clipboard register `a`, expected `+` or `*`");

		let settings = Value::Map(vec![(Value::from("scrol"), Value::Array(Vec::new()))]);
		let error = decode("setup", vec![settings]).unwrap_err();
		assert!(error.contains("unknown field `scrol`"), "{error}");

		let request = decode("unknown", vec![]).unwrap();
		assert!(matches!(&request, Request::Other { method, .. } if method == "unknown"), "{request:?}");
	}
}
