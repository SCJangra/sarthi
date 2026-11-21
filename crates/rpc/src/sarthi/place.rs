//! Where an external window goes, as `require('sarthi').api` takes it in `sarthi.place`

use schemars::JsonSchema;
use serde::Deserialize;

use crate::Window;
use crate::sarthi::notifications::optional_window_number;

/// Where Sarthi places an external window, over Neovim's screen (`sarthi.place`).
///
/// `{ center = true }` centers it, as it is until placed; otherwise it's placed like a floating window, with its
/// `anchor` corner at `row`, `col` of what it's `relative` to. Like a floating window with a border, whose frame, its
/// padding and outline ([padding](#sarthi-window.padding), [outline](#sarthi-window.outline)), takes the border's place: the frame's
/// `anchor` corner is at `row`, `col`, and the window's cells are inside it. `row` and `col` are whole cells, a
/// fraction dropped. A window it's relative to counts from its text, inside its frame, border and winbar, like Neovim's
/// floats do. Like those, it's kept on the screen with its frame, above the command line, and with `fixed` not moved
/// sideways unless anchored on the right. `offset` moves it further by logical pixels.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default, deny_unknown_fields)]
pub struct WindowPlace {
	/// Centered over the screen
	pub center: Option<bool>,
	/// What `row` and `col` count from
	pub relative: Option<PlaceRelative>,

	/// The window it's relative to, for `relative = 'win'`; the current window if left out, like for floats
	#[serde(deserialize_with = "optional_window_number")]
	pub win: Option<Window>,

	/// The corner at `row`, `col`
	pub anchor: Option<PlaceAnchor>,
	/// Rows from what it's relative to
	pub row: Option<f64>,
	/// Columns from what it's relative to
	pub col: Option<f64>,

	/// Moves it further, in logical pixels, e.g. past another window's padding: `{ x = …, y = … }`
	pub offset: Option<PlaceOffset>,

	/// Not moved sideways to stay on the screen, unless anchored on the right, like a float's `fixed`
	pub fixed: Option<bool>,
}

/// `sarthi.place.offset`, right and down in logical pixels, or left and up if negative; either left out is 0
#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default, deny_unknown_fields)]
pub struct PlaceOffset {
	/// Right, or left if negative
	pub x: f64,
	/// Down, or up if negative
	pub y: f64,
}

/// What a window's `row` and `col` count from, named like a float's `relative`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(inline)]
pub enum PlaceRelative {
	/// The screen's top-left corner
	Editor,
	/// Window `win`'s text
	Win,
	/// The cursor
	Cursor,
}

/// Which corner is at `row`, `col`, named like Neovim's `anchor`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[schemars(inline)]
pub enum PlaceAnchor {
	/// The top-left corner
	#[serde(rename = "NW")]
	NorthWest,
	/// The top-right corner
	#[serde(rename = "NE")]
	NorthEast,
	/// The bottom-left corner
	#[serde(rename = "SW")]
	SouthWest,
	/// The bottom-right corner
	#[serde(rename = "SE")]
	SouthEast,
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{PlaceAnchor, PlaceOffset, PlaceRelative, WindowPlace};
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

		// An offset, either side left out
		let offset = Value::Map(vec![(Value::from("y"), Value::from(16))]);
		let place = decode(vec![("relative", Value::from("editor")), ("offset", offset)]).unwrap();
		assert_eq!(place.offset, Some(PlaceOffset { x: 0.0, y: 16.0 }));
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
