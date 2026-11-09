use nvim::CursorShape;
use renderer::{BarOrientation, CellLayer, Cursor, Decoration, RenderCell, TextStyle};

use crate::cursor::CursorStyle;
use crate::grid::Grid;
use crate::highlight::HighlightState;

/// A snapshot of grid `id` for the renderer, with every cell's highlight resolved and, unless `cursor` is `None`,
/// the cursor at the grid's cursor position. Without multigrid, Neovim draws everything to grid 1, which fills
/// the window.
pub fn cell_layer(id: u32, grid: &Grid, highlights: &HighlightState, cursor: Option<CursorStyle>) -> CellLayer {
	let cells = grid
		.cells
		.iter()
		.map(|cell| {
			let style = highlights.resolve(cell.hl_id);
			RenderCell {
				text: cell.text,
				style: TextStyle { foreground: style.foreground, bold: style.bold, italic: style.italic },
				background: style.background,
				decoration: Decoration {
					underline: style.underline,
					strikethrough: style.strikethrough,
					overline: style.overline,
					underline_color: style.special,
				},
			}
		})
		.collect();

	let mut layer = CellLayer {
		id: u64::from(id),
		row: 0,
		col: 0,
		width: grid.width as u32,
		height: grid.height as u32,
		cells,
		cursor: None,
		scroll: None,
	};
	if let Some(style) = cursor {
		draw_cursor(&mut layer, grid.cursor_row, grid.cursor_col, style, highlights);
	}
	layer
}

/// Gives `layer` its cursor, at `row`, `col`, in `style`. The colors come from the cursor's highlight, or are the
/// cell's swapped when it has none (`:help ui-event-mode_info_set`).
fn draw_cursor(layer: &mut CellLayer, row: usize, col: usize, style: CursorStyle, highlights: &HighlightState) {
	let width = layer.width as usize;
	let index = row * width + col;
	let Some(&cell) = layer.cells.get(index).filter(|_| col < width) else { return };

	// A double-width character also covers the next cell, which is empty.
	let double_width = col + 1 < width && layer.cells[index + 1].text.is_empty();

	let (text_color, color) = if style.attr_id == 0 {
		(cell.background, cell.style.foreground)
	} else {
		let colors = highlights.resolve(style.attr_id);
		(colors.foreground, colors.background)
	};

	let bar = |orientation| renderer::CursorShape::Bar { orientation, percentage: style.percentage };
	let shape = match style.shape {
		CursorShape::Block => renderer::CursorShape::Block,
		CursorShape::Vertical => bar(BarOrientation::Vertical),
		CursorShape::Horizontal => bar(BarOrientation::Horizontal),
	};

	let (row, col, width) = (row as u32, col as u32, if double_width { 2 } else { 1 });
	layer.cursor = Some(Cursor { row, col, width, shape, color, text_color });
}

#[cfg(test)]
mod tests {
	use nvim::{CellText, CursorShape, DefaultColorsSetEvent, GridCell, HlAttrDefineEvent, HlAttrs};
	use renderer::{BarOrientation, Cursor, TextStyle};

	use super::cell_layer;
	use crate::cursor::CursorStyle;
	use crate::grid::Grid;
	use crate::highlight::HighlightState;
	use crate::highlight::tests::attrs;

	const FG: u32 = 0xC7C7C7;
	const BG: u32 = 0x1C1C1C;
	const CURSOR_FG: u32 = 0x000001;
	const CURSOR_BG: u32 = 0x00FF00;

	fn highlights() -> HighlightState {
		let mut state = HighlightState::new();
		let colors = DefaultColorsSetEvent { rgb_fg: FG, rgb_bg: BG, rgb_sp: 0xFF0000, cterm_fg: 0, cterm_bg: 0 };
		state.set_default_colors(vec![colors]);

		let bold = HlAttrs { foreground: Some(0x112233), bold: true, ..attrs() };
		let reverse = HlAttrs { reverse: true, italic: true, ..attrs() };
		let cursor = HlAttrs { foreground: Some(CURSOR_FG), background: Some(CURSOR_BG), ..attrs() };
		state.set_attrs(vec![
			HlAttrDefineEvent { id: 1, rgb_attr: bold.clone(), cterm_attr: bold, info: vec![] },
			HlAttrDefineEvent { id: 2, rgb_attr: reverse.clone(), cterm_attr: reverse, info: vec![] },
			HlAttrDefineEvent { id: 3, rgb_attr: cursor.clone(), cterm_attr: cursor, info: vec![] },
		]);
		state
	}

	fn cell(text: &str, hl_id: Option<u32>) -> GridCell {
		GridCell { text: CellText::from(text).unwrap(), hl_id, repeat: None }
	}

	#[test]
	fn cells_get_resolved_styles() {
		let mut grid = Grid::new(3, 2);
		grid.set_line(1, 0, &[cell("a", None), cell("b", Some(1)), cell("c", Some(2))]);

		let layer = cell_layer(1, &grid, &highlights(), None);
		assert_eq!((layer.id, layer.row, layer.col, layer.width, layer.height), (1, 0, 0, 3, 2));
		assert_eq!(layer.cells.len(), 6);

		let plain = TextStyle { foreground: FG, bold: false, italic: false };
		assert_eq!(layer.cells[0].text.as_str(), " ");
		assert_eq!(layer.cells[0].style, plain);

		let row: Vec<_> = layer.cells[3..]
			.iter()
			.map(|cell| (cell.text.as_str(), cell.style, cell.background))
			.collect();
		assert_eq!(
			row,
			[
				("a", plain, BG),
				("b", TextStyle { foreground: 0x112233, bold: true, italic: false }, BG),
				// Reverse: the default colors swap.
				("c", TextStyle { foreground: BG, bold: false, italic: true }, FG),
			],
		);
	}

	fn style(shape: CursorShape, percentage: u32, attr_id: u32) -> CursorStyle {
		CursorStyle { shape, percentage, attr_id }
	}

	/// A 4×1 grid "ab漢" (the last cell is the right half of 漢) with the cursor at `col`
	fn grid_with_cursor(col: usize) -> Grid {
		let mut grid = Grid::new(4, 1);
		grid.set_line(0, 0, &[cell("a", None), cell("b", Some(1)), cell("漢", Some(0)), cell("", None)]);
		grid.set_cursor(0, col);
		grid
	}

	fn colors(layer: &renderer::CellLayer) -> Vec<(u32, u32)> {
		layer
			.cells
			.iter()
			.map(|cell| (cell.style.foreground, cell.background))
			.collect()
	}

	fn cursor(col: u32, width: u32, shape: renderer::CursorShape, color: u32, text_color: u32) -> Option<Cursor> {
		Some(Cursor { row: 0, col, width, shape, color, text_color })
	}

	#[test]
	fn block_cursor_swaps_the_cell_colors() {
		let layer = cell_layer(1, &grid_with_cursor(1), &highlights(), Some(style(CursorShape::Block, 0, 0)));
		// "b" is 0x112233 on the default background, so the cursor is the default background on 0x112233.
		assert_eq!(layer.cursor, cursor(1, 1, renderer::CursorShape::Block, 0x112233, BG));
		// The cells keep their own colors: the renderer draws the cursor over them.
		assert_eq!(colors(&layer), [(FG, BG), (0x112233, BG), (FG, BG), (FG, BG)]);
	}

	#[test]
	fn block_cursor_uses_its_highlight() {
		let layer = cell_layer(1, &grid_with_cursor(0), &highlights(), Some(style(CursorShape::Block, 0, 3)));
		assert_eq!(layer.cursor, cursor(0, 1, renderer::CursorShape::Block, CURSOR_BG, CURSOR_FG));
	}

	#[test]
	fn block_cursor_covers_both_halves_of_a_double_width_character() {
		let layer = cell_layer(1, &grid_with_cursor(2), &highlights(), Some(style(CursorShape::Block, 0, 3)));
		assert_eq!(layer.cursor.map(|cursor| (cursor.col, cursor.width)), Some((2, 2)));
	}

	#[test]
	fn bar_cursors() {
		let bar = |orientation| renderer::CursorShape::Bar { orientation, percentage: 25 };
		let layer = cell_layer(1, &grid_with_cursor(1), &highlights(), Some(style(CursorShape::Vertical, 25, 0)));
		// The bar takes the swapped background, i.e. the cell's text color.
		assert_eq!(layer.cursor, cursor(1, 1, bar(BarOrientation::Vertical), 0x112233, BG));

		let layer = cell_layer(1, &grid_with_cursor(2), &highlights(), Some(style(CursorShape::Horizontal, 25, 3)));
		assert_eq!(layer.cursor, cursor(2, 2, bar(BarOrientation::Horizontal), CURSOR_BG, CURSOR_FG));
	}

	#[test]
	fn no_cursor_when_hidden_or_outside_the_grid() {
		assert_eq!(cell_layer(1, &grid_with_cursor(1), &highlights(), None).cursor, None);

		let mut grid = grid_with_cursor(0);
		grid.cursor_col = 7;
		assert_eq!(cell_layer(1, &grid, &highlights(), Some(style(CursorShape::Block, 0, 0))).cursor, None);
	}
}
