//! Where external windows go: sarthi places them itself, since Neovim leaves that to the UI (`:help ui-multigrid`)

use renderer::CellSize;
use rpc::neovim::Window;

/// Where an external window goes. Rows and columns are in cells of the screen (grid 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
	/// Centered over the screen, until placed otherwise
	Center,

	/// Like a floating window (`nvim_open_win()`'s `relative`, `anchor`, `row` and `col`): its `anchor` corner at `row`,
	/// `col` from what it's relative to, moved further by `offset` (right and down, in logical pixels), kept on the screen
	Relative { relative: Relative, anchor: Anchor, row: f64, col: f64, offset: [f64; 2] },
}

/// What a [`Place::Relative`] window's `row` and `col` count from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relative {
	/// The screen's top-left corner
	Editor,

	/// Window `win`'s top-left corner, wherever the window is. While it isn't shown, the external window is centered.
	Win(Window),

	/// The cursor, where it was when placed: the window doesn't follow it, like a floating window doesn't
	Cursor,
}

/// Which corner of a [`Place::Relative`] window is at its `row` and `col`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Anchor {
	#[default]
	NorthWest,
	NorthEast,
	SouthWest,
	SouthEast,
}

impl Anchor {
	/// Whether it's on the right, and whether at the bottom
	pub(crate) fn sides(self) -> (bool, bool) {
		match self {
			Self::NorthWest => (false, false),
			Self::NorthEast => (true, false),
			Self::SouthWest => (false, true),
			Self::SouthEast => (true, true),
		}
	}
}

/// The top-left corner, in pixels, of a window `size` pixels large (width, height) placed by `place` on a screen
/// `screen` pixels large, whose cells are `cell`. `origin` gives a window's top-left corner in pixels, if it's shown, and
/// its cells, which `row` and `col` relative to it count in. `scale` physical pixels make a logical one, which the
/// offset is in. A [`Relative::Cursor`] place must be made relative to the editor before (see [`at_cursor`]).
pub(crate) fn position(
	place: Place,
	size: [u32; 2],
	screen: [u32; 2],
	cell: CellSize,
	scale: f64,
	origin: impl Fn(Window) -> Option<([u32; 2], CellSize)>,
) -> [u32; 2] {
	let centered = [0, 1].map(|axis| screen[axis].saturating_sub(size[axis]) / 2);
	let Place::Relative { relative, anchor, row, col, offset } = place else { return centered };
	let ([base_x, base_y], cell) = match relative {
		Relative::Editor | Relative::Cursor => ([0, 0], cell),
		Relative::Win(win) => {
			let Some(origin) = origin(win) else { return centered };
			origin
		},
	};

	// The anchor corner, in pixels
	let x = f64::from(base_x) + col * f64::from(cell.width) + offset[0] * scale;
	let y = f64::from(base_y) + row * f64::from(cell.height) + offset[1] * scale;
	let (east, south) = anchor.sides();
	let left = if east { x - f64::from(size[0]) } else { x };
	let top = if south { y - f64::from(size[1]) } else { y };

	// On the screen as far as it fits, like Neovim keeps floating windows on it
	let keep =
		|start: f64, size: u32, screen: u32| start.round().clamp(0.0, f64::from(screen.saturating_sub(size))) as u32;
	[keep(left, size[0], screen[0]), keep(top, size[1], screen[1])]
}

/// `place` with a [`Relative::Cursor`] made relative to the editor, for the cursor at `cursor` (row, column of the
/// screen): where it is when placed
pub(crate) fn at_cursor(place: Place, cursor: (u32, u32)) -> Place {
	match place {
		Place::Relative { relative: Relative::Cursor, anchor, row, col, offset } => Place::Relative {
			relative: Relative::Editor,
			anchor,
			offset,
			row: f64::from(cursor.0) + row,
			col: f64::from(cursor.1) + col,
		},
		place => place,
	}
}

#[cfg(test)]
mod tests {
	use renderer::CellSize;
	use rpc::neovim::Window;

	use super::{Anchor, Place, Relative, at_cursor, position};

	const CELL: CellSize = CellSize { width: 10, height: 20 };

	/// A 100×200 pixel screen (10×10 cells), with window 7 at row 2, column 3, and window 8, with cells of its own
	/// (5×10), at 15, 25 pixels
	fn placed(place: Place, size: [u32; 2]) -> [u32; 2] {
		position(place, size, [100, 200], CELL, 2.0, |win| match win {
			Window(7) => Some(([30, 40], CELL)),
			Window(8) => Some(([15, 25], CellSize { width: 5, height: 10 })),
			_ => None,
		})
	}

	fn relative(relative: Relative, anchor: Anchor, row: f64, col: f64) -> Place {
		Place::Relative { relative, anchor, row, col, offset: [0.0; 2] }
	}

	#[test]
	fn centered() {
		assert_eq!(placed(Place::Center, [40, 60]), [30, 70]);
		// Larger than the screen: from its top-left corner
		assert_eq!(placed(Place::Center, [140, 260]), [0, 0]);
	}

	#[test]
	fn relative_to_the_editor_by_each_corner() {
		let at = |anchor| placed(relative(Relative::Editor, anchor, 5.0, 6.0), [20, 40]);
		// The corner at (60, 100)
		assert_eq!(at(Anchor::NorthWest), [60, 100]);
		assert_eq!(at(Anchor::NorthEast), [40, 100]);
		assert_eq!(at(Anchor::SouthWest), [60, 60]);
		assert_eq!(at(Anchor::SouthEast), [40, 60]);
		// Fractional cells, rounded to whole pixels
		assert_eq!(placed(relative(Relative::Editor, Anchor::NorthWest, 0.5, 0.25), [20, 40]), [3, 10]);
	}

	#[test]
	fn kept_on_the_screen() {
		// Past the bottom-right corner, and before the top-left one
		assert_eq!(placed(relative(Relative::Editor, Anchor::NorthWest, 9.0, 9.0), [20, 40]), [80, 160]);
		assert_eq!(placed(relative(Relative::Editor, Anchor::SouthEast, 0.0, 0.0), [20, 40]), [0, 0]);
	}

	#[test]
	fn moved_by_an_offset() {
		// 3 and 5 logical pixels, at 2 physical pixels in one
		let offset = Place::Relative {
			relative: Relative::Editor,
			anchor: Anchor::NorthWest,
			row: 1.0,
			col: 2.0,
			offset: [3.0, 5.0],
		};
		assert_eq!(placed(offset, [20, 40]), [2 * 10 + 6, 20 + 10]);
		// Still kept on the screen
		let far = Place::Relative {
			relative: Relative::Editor,
			anchor: Anchor::NorthWest,
			row: 0.0,
			col: 0.0,
			offset: [-50.0, 0.0],
		};
		assert_eq!(placed(far, [20, 40]), [0, 0]);
	}

	#[test]
	fn relative_to_a_window() {
		// One row below window 7's top-left corner (2, 3)
		assert_eq!(placed(relative(Relative::Win(Window(7)), Anchor::NorthWest, 1.0, 0.0), [20, 40]), [30, 60]);
		// In window 8's own cells: 2 rows and 3 columns of 5×10
		assert_eq!(placed(relative(Relative::Win(Window(8)), Anchor::NorthWest, 2.0, 3.0), [20, 40]), [30, 45]);
		// A window that isn't shown: centered
		assert_eq!(placed(relative(Relative::Win(Window(9)), Anchor::NorthWest, 1.0, 0.0), [20, 40]), [40, 80]);
	}

	#[test]
	fn relative_to_the_cursor_where_it_was() {
		let place = at_cursor(relative(Relative::Cursor, Anchor::NorthWest, 1.0, 0.0), (4, 2));
		assert_eq!(place, relative(Relative::Editor, Anchor::NorthWest, 5.0, 2.0));
		assert_eq!(placed(place, [20, 40]), [20, 100]);
		// Other places stay as they are.
		assert_eq!(at_cursor(Place::Center, (4, 2)), Place::Center);
	}
}
