//! Where an external window goes, as `require('sarthi').api` takes it in `sarthi.place`

use serde::Deserialize;

use crate::Window;
use crate::sarthi::notifications::optional_window_number;

/// `sarthi.place`: `{ center = true }`, or `{ relative = …, anchor = …, row = …, col = … }` like a floating window's
/// config. Which of them it is, sarthi checks once decoded.
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default, deny_unknown_fields)]
pub struct WindowPlace {
	pub center: Option<bool>,
	pub relative: Option<PlaceRelative>,

	/// The window it's relative to, for `relative = 'win'`; the current window if left out, like for floats
	#[serde(deserialize_with = "optional_window_number")]
	pub win: Option<Window>,

	pub anchor: Option<PlaceAnchor>,
	pub row: Option<f64>,
	pub col: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceRelative {
	Editor,
	Win,
	Cursor,
}

/// Which corner is at `row`, `col`, named like Neovim's `anchor`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum PlaceAnchor {
	#[serde(rename = "NW")]
	NorthWest,
	#[serde(rename = "NE")]
	NorthEast,
	#[serde(rename = "SW")]
	SouthWest,
	#[serde(rename = "SE")]
	SouthEast,
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{PlaceAnchor, PlaceRelative, WindowPlace};
	use crate::Window;

	fn decode(fields: Vec<(&str, Value)>) -> Result<WindowPlace, String> {
		// From msgpack, like Neovim sends it
		let map = Value::Map(fields.into_iter().map(|(key, value)| (Value::from(key), value)).collect());
		rmp_serde::from_slice(&rmp_serde::to_vec(&map).unwrap()).map_err(|error| error.to_string())
	}

	#[test]
	fn places() {
		let centered = decode(vec![("center", Value::from(true))]).unwrap();
		assert_eq!(centered, WindowPlace { center: Some(true), ..WindowPlace::default() });

		let relative = decode(vec![
			("relative", Value::from("win")),
			("win", Value::from(1000)),
			("anchor", Value::from("SE")),
			("row", Value::from(2)),
			("col", Value::from(1.5)),
		])
		.unwrap();
		let expected = WindowPlace {
			relative: Some(PlaceRelative::Win),
			win: Some(Window(1000)),
			anchor: Some(PlaceAnchor::SouthEast),
			row: Some(2.0),
			col: Some(1.5),
			..WindowPlace::default()
		};
		assert_eq!(relative, expected);
	}

	#[test]
	fn mistakes() {
		assert!(
			decode(vec![("relative", Value::from("middle"))])
				.unwrap_err()
				.contains("unknown variant `middle`")
		);
		assert!(
			decode(vec![("centered", Value::from(true))])
				.unwrap_err()
				.contains("unknown field `centered`")
		);
		assert!(
			decode(vec![("anchor", Value::from("nw"))])
				.unwrap_err()
				.contains("unknown variant `nw`")
		);
	}
}
