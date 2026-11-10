//! What the clipboard provider in `runtime/lua/nvui/clipboard.lua` sends and gets back

use serde::{Deserialize, Serialize};

/// Text Neovim copied: its lines, and its register type (`v`, `V` or a block like `^V10`)
#[derive(Debug, Clone, PartialEq)]
pub struct Copied {
	pub lines: Vec<String>,
	pub regtype: String,
}

/// What a paste returns to Neovim: the lines and their type when nvui copied them, so e.g. lines copied linewise
/// paste linewise; otherwise only the lines, and Neovim takes a trailing empty line as linewise.
#[derive(Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Paste {
	Typed((Vec<String>, String)),
	Lines(Vec<String>),
}

/// The `+` (clipboard) or `*` (primary selection) register, decoded from the name Neovim gives it
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub enum Register {
	Clipboard,
	Primary,
}

impl TryFrom<String> for Register {
	type Error = String;

	fn try_from(name: String) -> Result<Self, Self::Error> {
		match name.as_str() {
			"+" => Ok(Self::Clipboard),
			"*" => Ok(Self::Primary),
			_ => Err(format!("no clipboard register `{name}`, expected `+` or `*`")),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::Paste;

	#[test]
	fn paste_encodes_like_neovim_expects() {
		let lines = |lines: &[&str]| lines.iter().map(|line| line.to_string()).collect::<Vec<_>>();
		let typed = Paste::Typed((lines(&["a"]), String::from("v")));
		let encoded: rmpv::Value = rmpv::ext::to_value(&typed).unwrap();
		assert_eq!(encoded.to_string(), r#"[["a"], "v"]"#);
		let encoded: rmpv::Value = rmpv::ext::to_value(Paste::Lines(lines(&["a"]))).unwrap();
		assert_eq!(encoded.to_string(), r#"["a"]"#);
	}
}
