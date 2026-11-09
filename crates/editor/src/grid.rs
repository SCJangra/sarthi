#![allow(dead_code)]

use nvim::{CellText, GridCell as NvimGridCell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCell {
	pub text: CellText,
	/// Highlight from `hl_attr_define`; 0 is the default highlight
	pub hl_id: u32,
}

impl GridCell {
	/// An empty cell, shown as a space
	pub fn blank() -> Self {
		Self { text: CellText::from(" ").expect("a space fits in a cell"), hl_id: 0 }
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
	pub width: usize,
	pub height: usize,
	pub cells: Vec<GridCell>,
	pub cursor_row: usize,
	pub cursor_col: usize,
}

impl Grid {
	pub fn new(width: usize, height: usize) -> Self {
		let mut grid =
			Self { width: 0, height: 0, cells: Vec::new(), cursor_row: 0, cursor_col: 0 };
		grid.resize(width, height);
		grid
	}

	/// Resizes the grid, keeping the cells that still fit; new cells are blank. Neovim only redraws what changed
	/// afterwards, e.g. after a split shrinks a window, so the rest must stay.
	pub fn resize(&mut self, width: usize, height: usize) {
		let mut cells = vec![GridCell::blank(); width * height];
		let kept = self.width.min(width);
		for row in 0..self.height.min(height) {
			let old = row * self.width;
			cells[row * width..row * width + kept].copy_from_slice(&self.cells[old..old + kept]);
		}

		self.cells = cells;
		self.width = width;
		self.height = height;
		self.cursor_row = self.cursor_row.min(height.saturating_sub(1));
		self.cursor_col = self.cursor_col.min(width.saturating_sub(1));
	}

	pub fn clear(&mut self) {
		self.cells.fill(GridCell::blank());
		self.cursor_row = 0;
		self.cursor_col = 0;
	}

	pub fn set_line(&mut self, row: usize, col_start: usize, cells: &[NvimGridCell]) {
		if row >= self.height || self.width == 0 {
			return;
		}

		let mut col = col_start;

		// Cells only carry `hl_id` when it changes; the rest reuse the last one seen.
		let mut hl_id = 0;

		for cell in cells {
			hl_id = cell.hl_id.unwrap_or(hl_id);

			let repeat = cell.repeat.unwrap_or(1) as usize;
			for _ in 0..repeat {
				if col >= self.width {
					return;
				}
				let index = self.cell_index(row, col);
				self.cells[index] = GridCell { text: cell.text, hl_id };
				col += 1;
			}
		}
	}

	/// Moves the cells in rows `top..bot` and columns `left..right` up by `rows`, or down if `rows` is negative
	/// (`grid_scroll`). The rows that open up keep their old cells: Neovim redraws them right after.
	pub fn scroll(&mut self, top: usize, bot: usize, left: usize, right: usize, rows: isize) {
		let (bot, right) = (bot.min(self.height), right.min(self.width));
		if rows == 0 || left >= right {
			return;
		}

		let distance = rows.unsigned_abs();
		let copy_row = |grid: &mut Self, dst: usize| {
			let src = if rows > 0 { dst + distance } else { dst - distance };
			let (start, dst) = (grid.cell_index(src, left), grid.cell_index(dst, left));
			grid.cells.copy_within(start..start + (right - left), dst);
		};

		// Each row is read before it's overwritten: moving up copies top to bottom, moving down bottom to top.
		if rows > 0 {
			(top..bot.saturating_sub(distance)).for_each(|dst| copy_row(self, dst));
		} else {
			(top + distance..bot).rev().for_each(|dst| copy_row(self, dst));
		}
	}

	pub fn set_cursor(&mut self, row: usize, col: usize) {
		if row >= self.height || col >= self.width {
			return;
		}
		self.cursor_row = row;
		self.cursor_col = col;
	}

	fn cell_index(&self, row: usize, col: usize) -> usize {
		(row * self.width) + col
	}

	fn cell_count(&self) -> usize {
		self.width * self.height
	}
}

#[cfg(test)]
mod tests {
	use super::{Grid, GridCell};
	use nvim::{CellText, GridCell as NvimGridCell};

	#[test]
	fn new_grid() {
		let grid = Grid::new(3, 2);

		assert_eq!(grid.width, 3);
		assert_eq!(grid.height, 2);
		assert_eq!(grid.cells.len(), 6);
		assert!(grid.cells.iter().all(|cell| *cell == GridCell::blank()));
	}

	#[test]
	fn resize_keeps_the_cells_that_fit() {
		let mut grid = grid_of(&["abc", "def", "ghi"]);
		grid.set_cursor(2, 2);

		grid.resize(2, 2);
		assert_eq!(rows_of(&grid), ["ab", "de"]);
		assert_eq!((grid.cursor_row, grid.cursor_col), (1, 1));

		// Growing again, the new cells are blank.
		grid.resize(4, 3);
		assert_eq!(rows_of(&grid), ["ab  ", "de  ", "    "]);
	}

	#[test]
	fn clear() {
		let mut grid = Grid::new(2, 2);

		grid.cells[0] = GridCell { text: CellText::from("x").unwrap(), hl_id: 1 };
		grid.cells[3] = GridCell { text: CellText::from("y").unwrap(), hl_id: 2 };
		grid.clear();

		assert!(grid.cells.iter().all(|cell| *cell == GridCell::blank()));
	}

	#[test]
	fn set_line() {
		let mut grid = Grid::new(5, 2);
		let cells = vec![
			NvimGridCell { text: CellText::from("a").unwrap(), hl_id: Some(1), repeat: Some(2) },
			NvimGridCell { text: CellText::from("b").unwrap(), hl_id: None, repeat: None },
			NvimGridCell { text: CellText::from("c").unwrap(), hl_id: Some(3), repeat: Some(10) },
		];

		grid.set_line(1, 1, &cells);

		let expected = vec![
			GridCell::blank(),
			GridCell::blank(),
			GridCell::blank(),
			GridCell::blank(),
			GridCell::blank(),
			GridCell::blank(),
			GridCell { text: CellText::from("a").unwrap(), hl_id: 1 },
			GridCell { text: CellText::from("a").unwrap(), hl_id: 1 },
			GridCell { text: CellText::from("b").unwrap(), hl_id: 1 },
			GridCell { text: CellText::from("c").unwrap(), hl_id: 3 },
		];

		assert_eq!(grid.cells, expected);
	}

	#[test]
	fn set_line_reuses_last_hl_id() {
		let mut grid = Grid::new(6, 1);
		let cells = vec![
			NvimGridCell { text: CellText::from("a").unwrap(), hl_id: Some(4), repeat: None },
			NvimGridCell { text: CellText::from("b").unwrap(), hl_id: None, repeat: Some(2) },
			NvimGridCell { text: CellText::from("c").unwrap(), hl_id: None, repeat: None },
			NvimGridCell { text: CellText::from("d").unwrap(), hl_id: Some(0), repeat: None },
			NvimGridCell { text: CellText::from("e").unwrap(), hl_id: None, repeat: None },
		];

		grid.set_line(0, 0, &cells);

		let hl_ids: Vec<u32> = grid.cells.iter().map(|cell| cell.hl_id).collect();
		assert_eq!(hl_ids, [4, 4, 4, 4, 0, 0]);
	}

	/// A grid with one row per string, one character per cell
	fn grid_of(rows: &[&str]) -> Grid {
		let mut grid = Grid::new(rows[0].len(), rows.len());
		for (row, text) in rows.iter().enumerate() {
			let cells: Vec<_> = text
				.chars()
				.map(|c| NvimGridCell { text: CellText::from(&c.to_string()).unwrap(), hl_id: None, repeat: None })
				.collect();
			grid.set_line(row, 0, &cells);
		}
		grid
	}

	fn rows_of(grid: &Grid) -> Vec<String> {
		grid.cells
			.chunks(grid.width)
			.map(|row| row.iter().map(|cell| cell.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn scroll_up() {
		let mut grid = grid_of(&["abc", "def", "ghi", "jkl"]);
		grid.scroll(0, 4, 0, 3, 1);
		// The last row opened up and keeps its cells until Neovim redraws it.
		assert_eq!(rows_of(&grid), ["def", "ghi", "jkl", "jkl"]);
	}

	#[test]
	fn scroll_down() {
		let mut grid = grid_of(&["abc", "def", "ghi", "jkl"]);
		grid.scroll(0, 4, 0, 3, -2);
		assert_eq!(rows_of(&grid), ["abc", "def", "abc", "def"]);
	}

	#[test]
	fn scroll_partial_region() {
		let mut grid = grid_of(&["abcd", "efgh", "ijkl", "mnop"]);
		// Rows 1..3, columns 1..3, up by one: like a window next to a split, above the status line
		grid.scroll(1, 3, 1, 3, 1);
		assert_eq!(rows_of(&grid), ["abcd", "ejkh", "ijkl", "mnop"]);

		let mut grid = grid_of(&["abcd", "efgh", "ijkl", "mnop"]);
		grid.scroll(1, 4, 1, 3, -1);
		assert_eq!(rows_of(&grid), ["abcd", "efgh", "ifgl", "mjkp"]);
	}

	#[test]
	fn scroll_by_region_height_or_more_moves_nothing() {
		let mut grid = grid_of(&["abc", "def", "ghi"]);
		grid.scroll(0, 2, 0, 3, 2);
		grid.scroll(0, 3, 0, 3, -5);
		grid.scroll(0, 3, 0, 3, 0);
		assert_eq!(rows_of(&grid), ["abc", "def", "ghi"]);
	}

	#[test]
	fn scroll_clamps_region_to_grid() {
		let mut grid = grid_of(&["abc", "def", "ghi"]);
		grid.scroll(0, 10, 0, 10, 1);
		assert_eq!(rows_of(&grid), ["def", "ghi", "ghi"]);

		grid.scroll(0, 3, 5, 10, 1);
		assert_eq!(rows_of(&grid), ["def", "ghi", "ghi"]);
	}

	#[test]
	fn set_cursor() {
		let mut grid = Grid::new(3, 2);

		grid.set_cursor(1, 2);
		assert_eq!(grid.cursor_row, 1);
		assert_eq!(grid.cursor_col, 2);

		grid.set_cursor(5, 1);
		assert_eq!(grid.cursor_row, 1);
		assert_eq!(grid.cursor_col, 2);
	}
}
