//! `sarthi.place` from `require('sarthi').api`, checked and turned into where the editor puts an external window

use editor::{Anchor, Place, Relative};
use rpc::sarthi::{PlaceAnchor, PlaceRelative, WindowPlace};

/// Where `place` puts an external window, or what's wrong with it. No place puts it back in the middle.
pub fn place(place: Option<&WindowPlace>) -> Result<Place, String> {
	let Some(place) = place else { return Ok(Place::Center) };
	let WindowPlace { center, relative, win, anchor, row, col, offset } = place;
	let relative_keys = win.is_some() || anchor.is_some() || row.is_some() || col.is_some() || offset.is_some();

	match (center, relative) {
		(Some(true), None) if !relative_keys => Ok(Place::Center),
		(Some(false), None) => Err(String::from("`sarthi.place.center` is only `true`; leave it out otherwise")),
		(None, Some(relative)) => {
			let relative = match (relative, win) {
				(PlaceRelative::Editor, None) => Relative::Editor,
				(PlaceRelative::Cursor, None) => Relative::Cursor,
				(PlaceRelative::Win, Some(win)) => Relative::Win(*win),
				(PlaceRelative::Win, None) => return Err(String::from("`relative = 'win'` needs `win`")),
				(_, Some(_)) => return Err(String::from("`win` goes with `relative = 'win'`")),
			};
			let (row, col) = (row.unwrap_or(0.0), col.unwrap_or(0.0));
			if !row.is_finite() || !col.is_finite() {
				return Err(String::from("`row` and `col` must be numbers"));
			}
			let offset = offset.map_or([0.0; 2], |offset| [offset.x, offset.y]);
			if !offset.iter().all(|offset| offset.is_finite()) {
				return Err(String::from("`offset` must be numbers"));
			}
			let anchor = match anchor.unwrap_or(PlaceAnchor::NorthWest) {
				PlaceAnchor::NorthWest => Anchor::NorthWest,
				PlaceAnchor::NorthEast => Anchor::NorthEast,
				PlaceAnchor::SouthWest => Anchor::SouthWest,
				PlaceAnchor::SouthEast => Anchor::SouthEast,
			};
			Ok(Place::Relative { relative, anchor, row, col, offset })
		},
		(Some(_) | None, None) if relative_keys => {
			Err(String::from("`win`, `anchor`, `row`, `col` and `offset` go with `relative`"))
		},
		_ => Err(String::from("`sarthi.place` takes one of `center` or `relative`")),
	}
}

#[cfg(test)]
mod tests {
	use editor::{Anchor, Place, Relative};
	use rpc::neovim::Window;
	use rpc::sarthi::{PlaceAnchor, PlaceOffset, PlaceRelative, WindowPlace};

	use super::place;

	#[test]
	fn places() {
		assert_eq!(place(None), Ok(Place::Center));
		let centered = WindowPlace { center: Some(true), ..WindowPlace::default() };
		assert_eq!(place(Some(&centered)), Ok(Place::Center));

		// Like a float: by default its top-left corner at the top-left of what it's relative to
		let editor = WindowPlace { relative: Some(PlaceRelative::Editor), ..WindowPlace::default() };
		let expected = Place::Relative {
			relative: Relative::Editor,
			anchor: Anchor::NorthWest,
			row: 0.0,
			col: 0.0,
			offset: [0.0; 2],
		};
		assert_eq!(place(Some(&editor)), Ok(expected));
		let below = WindowPlace {
			relative: Some(PlaceRelative::Win),
			win: Some(Window(1000)),
			anchor: Some(PlaceAnchor::SouthEast),
			row: Some(2.0),
			col: Some(1.5),
			..WindowPlace::default()
		};
		let expected = Place::Relative {
			relative: Relative::Win(Window(1000)),
			anchor: Anchor::SouthEast,
			row: 2.0,
			col: 1.5,
			offset: [0.0; 2],
		};
		assert_eq!(place(Some(&below)), Ok(expected));
	}

	#[test]
	fn mistakes() {
		let error = |given: WindowPlace| place(Some(&given)).unwrap_err();
		assert!(error(WindowPlace::default()).contains("one of `center` or `relative`"));
		let both = WindowPlace { center: Some(true), relative: Some(PlaceRelative::Editor), ..WindowPlace::default() };
		assert!(error(both).contains("one of"));
		let centered_at = WindowPlace { center: Some(true), row: Some(1.0), ..WindowPlace::default() };
		assert!(error(centered_at).contains("go with `relative`"));
		let centered_by =
			WindowPlace { center: Some(true), offset: Some(PlaceOffset { x: 1.0, y: 0.0 }), ..WindowPlace::default() };
		assert!(error(centered_by).contains("go with `relative`"));
		let no_win = WindowPlace { relative: Some(PlaceRelative::Win), ..WindowPlace::default() };
		assert!(error(no_win).contains("needs `win`"));
		let stray_win =
			WindowPlace { relative: Some(PlaceRelative::Editor), win: Some(Window(1)), ..WindowPlace::default() };
		assert!(error(stray_win).contains("`win` goes with"));
		let nan = WindowPlace { relative: Some(PlaceRelative::Editor), row: Some(f64::NAN), ..WindowPlace::default() };
		assert!(error(nan).contains("numbers"));
		assert!(error(WindowPlace { center: Some(false), ..WindowPlace::default() }).contains("only `true`"));
	}
}
