//! What's under the mouse pointer: which grid and cell, for mouse input, and how the pointer looks over it.

use rpc::neovim::Grid;

use crate::{Editor, Placement};

/// How the mouse pointer looks, by what's under it (see [`Editor::pointer_at`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
	/// The default pointer.
	Arrow,

	/// Over text.
	Text,

	/// Over a separator between windows side by side, which drags left and right.
	ResizeColumns,

	/// Over a status line, which drags up and down.
	ResizeRows,
}

/// A cell of a grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPosition {
	/// The grid.
	pub grid: Grid,
	/// Row of the cell, from the grid's top.
	pub row: u32,
	/// Column of the cell, from the grid's left.
	pub col: u32,
}

impl Editor {
	/// The grid shown at `x`, `y` in pixels from the window's top-left corner, and the cell of it there: the topmost
	/// grid there that takes the mouse, so not a float made with `mouse = false` (`:help ui-multigrid`).
	pub fn grid_at(&self, x: f64, y: f64) -> Option<GridPosition> {
		if let Some(grid) = self.external_at(x, y) {
			let (row, col) = self.grid_cell(grid, x, y)?;
			return Some(GridPosition { grid, row: row as u32, col: col as u32 });
		}
		let (row, col) = self.screen_cell(x, y)?;
		self.grid_at_cell(row, col)
	}

	/// The grid shown at cell `row`, `col` of the screen, and that cell in it, among the grids on the screen's cells
	/// (not the external windows).
	fn grid_at_cell(&self, row: u32, col: u32) -> Option<GridPosition> {
		let screen_height = self.grids.get(&Grid::GLOBAL).map_or(0, |grid| grid.height);
		let covers = |id: Grid, top: u32, left: u32, bottom: u32| {
			let grid = &self.grids[&id];
			let right = left + grid.width;
			(top..bottom).contains(&row) && (left..right).contains(&col)
		};

		let over = self.stacked().into_iter().rev().find_map(|(id, placement)| {
			let (top, left, bottom) = match placement {
				Placement::Tiled { row, col } => (*row, *col, row + self.grids[&id].height),
				Placement::Float { row, col, mouse: true, .. } => (*row, *col, row + self.grids[&id].height),
				// Only its rows from `row` down show.
				Placement::Message { row, .. } => (*row, 0, screen_height),
				// An external window isn't on the screen's cells.
				Placement::Float { .. } | Placement::External { .. } | Placement::Hidden => return None,
			};
			covers(id, top, left, bottom).then_some((id, top, left))
		});
		let (grid, top, left) =
			over.or_else(|| covers(Grid::GLOBAL, 0, 0, screen_height).then_some((Grid::GLOBAL, 0, 0)))?;
		Some(GridPosition { grid, row: row - top, col: col - left })
	}

	/// The topmost external window at `x`, `y` in pixels, by its grid.
	fn external_at(&self, x: f64, y: f64) -> Option<Grid> {
		self.stacked().into_iter().rev().find_map(|(id, placement)| {
			let Placement::External { win, .. } = placement else { return None };
			let rect = self.external_rect(id, *win);
			let inside = (f64::from(rect.x)..f64::from(rect.right())).contains(&x)
				&& (f64::from(rect.y)..f64::from(rect.bottom())).contains(&y);
			inside.then_some(id)
		})
	}

	/// The cell of grid `id` at `x`, `y` in pixels from the window's top-left corner, as (row, column): negative or
	/// past the grid's edges when outside it, e.g. while dragging. `None` if the grid isn't shown.
	pub fn grid_cell(&self, id: Grid, x: f64, y: f64) -> Option<(i64, i64)> {
		let ([left, top], cell) = match self.placements.get(&id) {
			Some(Placement::External { win, .. }) => {
				let rect = self.external_rect(id, *win);
				([f64::from(rect.x), f64::from(rect.y)], self.cell_of(id))
			},
			_ => {
				let (row, col) = self.grid_origin(id)?;
				([f64::from(col * self.cell.width), f64::from(row * self.cell.height)], self.cell)
			},
		};
		let row = ((y - top) / f64::from(cell.height)).floor() as i64;
		let col = ((x - left) / f64::from(cell.width)).floor() as i64;
		Some((row, col))
	}

	/// The cell of the screen at `x`, `y` in pixels, as (row, column), if it's on the screen's side of the window's
	/// top-left corner.
	fn screen_cell(&self, x: f64, y: f64) -> Option<(u32, u32)> {
		let row = (y / f64::from(self.cell.height)).floor();
		let col = (x / f64::from(self.cell.width)).floor();
		(row >= 0.0 && col >= 0.0).then_some((row as u32, col as u32))
	}

	/// How the mouse pointer looks at `x`, `y` in pixels: text in windows, a resize arrow over separators and status
	/// lines, which can be dragged, and an arrow elsewhere, or while Neovim's mouse is off.
	pub fn pointer_at(&self, x: f64, y: f64) -> Pointer {
		if !self.mouse {
			return Pointer::Arrow;
		}
		match self.grid_at(x, y) {
			Some(GridPosition { grid: Grid::GLOBAL, row, col }) => self.border_at(row, col),
			Some(_) => Pointer::Text,
			None => Pointer::Arrow,
		}
	}

	/// What of grid 1 cell `row`, `col` is: a window's separator, right of it, or its status line, below it.
	fn border_at(&self, row: u32, col: u32) -> Pointer {
		let windows = self.placements.iter().filter_map(|(id, placement)| match placement {
			Placement::Tiled { row, col } => Some((*row, *col, self.grids.get(id)?)),
			_ => None,
		});
		let mut pointer = Pointer::Arrow;
		for (top, left, grid) in windows {
			let (bottom, right) = (top + grid.height, left + grid.width);
			// A status line wins where it meets a separator.
			if row == bottom && (left..=right).contains(&col) {
				return Pointer::ResizeRows;
			}
			if col == right && (top..bottom).contains(&row) {
				pointer = Pointer::ResizeColumns;
			}
		}
		pointer
	}

	/// The cell of the screen where grid `id`'s top-left cell is shown, if the grid is shown on the screen's cells (an
	/// external window isn't).
	pub(crate) fn grid_origin(&self, id: Grid) -> Option<(u32, u32)> {
		if id == Grid::GLOBAL {
			return self.grids.contains_key(&Grid::GLOBAL).then_some((0, 0));
		}
		match self.placements.get(&id)? {
			Placement::Tiled { row, col } | Placement::Float { row, col, .. } => Some((*row, *col)),
			Placement::Message { row, .. } => Some((*row, 0)),
			Placement::External { .. } | Placement::Hidden => None,
		}
	}
}
