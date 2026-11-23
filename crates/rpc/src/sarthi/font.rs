//! Fonts: the main one from `guifont`, and an external window's own, as `require('sarthi').api` takes it in
//! `sarthi.font`.

use schemars::JsonSchema;
use serde::Deserialize;

/// The fonts an external window's text is drawn with (`sarthi.font`). Either left out is `guifont`'s. The window is as
/// many of its own cells as its config says.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Font {
	/// Families in order: the first draws all text it has the characters for. `guifont`'s if left out.
	#[schemars(with = "Vec<String>")]
	pub family: FontFamily,

	/// In points, like `guifont`'s `:h`; `guifont`'s size if left out.
	#[schemars(with = "f32")]
	pub size: FontSize,
}

/// A font's families.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(untagged, expecting = "a list of font families")]
pub enum FontFamily {
	/// The main font's, from `guifont`.
	#[default]
	Main,
	/// These, in order: the first draws all text it has the characters for, the others, in order, the characters it
	/// lacks. Empty for sarthi's bundled font.
	Named(Vec<String>),
}

impl FontFamily {
	/// These families, or `main` for the main font's.
	pub fn or<'a>(&'a self, main: &'a [String]) -> &'a [String] {
		match self {
			Self::Main => main,
			Self::Named(families) => families,
		}
	}
}

/// A font's size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(untagged, expecting = "a size in points")]
pub enum FontSize {
	/// The main font's, from `guifont`.
	#[default]
	Main,
	/// In points.
	Points(f32),
}

impl FontSize {
	/// This size, or `main` for the main font's, in points.
	pub fn or(self, main: f32) -> f32 {
		match self {
			Self::Main => main,
			Self::Points(points) => points,
		}
	}
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{Font, FontFamily, FontSize};

	fn decode(fields: Vec<(&str, Value)>) -> Result<Font, String> {
		// From msgpack, like Neovim sends it.
		let map = Value::Map(fields.into_iter().map(|(key, value)| (Value::from(key), value)).collect());
		rmp_serde::from_slice(&rmp_serde::to_vec(&map).unwrap()).map_err(|error| error.to_string())
	}

	#[test]
	fn fonts() {
		let font =
			decode(vec![("family", Value::Array(vec![Value::from("A"), Value::from("B")])), ("size", Value::from(11))]);
		let family = FontFamily::Named(vec![String::from("A"), String::from("B")]);
		assert_eq!(font.unwrap(), Font { family, size: FontSize::Points(11.0) });

		// Left out, the main font's.
		assert_eq!(decode(vec![]).unwrap(), Font { family: FontFamily::Main, size: FontSize::Main });
	}

	#[test]
	fn mistakes() {
		assert!(
			decode(vec![("famly", Value::from("A"))])
				.unwrap_err()
				.contains("unknown field `famly`")
		);
		assert!(decode(vec![("family", Value::from(3))]).is_err());
		let error = decode(vec![("family", Value::from("A"))]).unwrap_err();
		assert!(error.contains("a list of font families"), "{error}");
		let error = decode(vec![("size", Value::from("big"))]).unwrap_err();
		assert!(error.contains("a size in points"), "{error}");
	}
}
