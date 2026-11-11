//! The fonts of an external window, as `require('sarthi').api` takes them in `sarthi.font`

use serde::Deserialize;

/// `sarthi.font`: the fonts and size an external window's text is drawn with
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default, deny_unknown_fields)]
pub struct WindowFont {
	/// A family, or families in order: the first draws all text it has the characters for. `guifont`'s if left out.
	pub family: Option<FontFamilies>,

	/// In points, like `guifont`'s `:h`; `guifont`'s size if left out
	pub size: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(untagged)]
pub enum FontFamilies {
	One(String),
	Many(Vec<String>),
}

impl FontFamilies {
	pub fn into_vec(self) -> Vec<String> {
		match self {
			Self::One(family) => vec![family],
			Self::Many(families) => families,
		}
	}
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{FontFamilies, WindowFont};

	fn decode(fields: Vec<(&str, Value)>) -> Result<WindowFont, String> {
		// From msgpack, like Neovim sends it
		let map = Value::Map(fields.into_iter().map(|(key, value)| (Value::from(key), value)).collect());
		rmp_serde::from_slice(&rmp_serde::to_vec(&map).unwrap()).map_err(|error| error.to_string())
	}

	#[test]
	fn fonts() {
		let one = decode(vec![("family", Value::from("Inter")), ("size", Value::from(11))]).unwrap();
		assert_eq!(one, WindowFont { family: Some(FontFamilies::One(String::from("Inter"))), size: Some(11.0) });
		assert_eq!(one.family.unwrap().into_vec(), ["Inter"]);

		let many = decode(vec![("family", Value::Array(vec![Value::from("A"), Value::from("B")]))]).unwrap();
		assert_eq!(many.family.unwrap().into_vec(), ["A", "B"]);
		assert_eq!(decode(vec![]).unwrap(), WindowFont::default());
	}

	#[test]
	fn mistakes() {
		assert!(
			decode(vec![("famly", Value::from("A"))])
				.unwrap_err()
				.contains("unknown field `famly`")
		);
		assert!(decode(vec![("family", Value::from(3))]).is_err());
	}
}
