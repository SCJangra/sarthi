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

	pub fn resize(&mut self, width: usize, height: usize) {
		self.width = width;
		self.height = height;
		self.cells.resize(self.cell_count(), GridCell::blank());
		self.clear();
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
