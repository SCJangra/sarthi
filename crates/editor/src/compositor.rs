use nvim::CursorShape;
use renderer::{BarCursor, BarOrientation, CellLayer, Decoration, RenderCell, TextStyle};

use crate::cursor::CursorStyle;
use crate::grid::Grid;
use crate::highlight::HighlightState;

/// A snapshot of grid `id` for the renderer, with every cell's highlight resolved and, unless `cursor` is `None`,
/// the cursor drawn at the grid's cursor position. Without multigrid, Neovim draws everything to grid 1, which fills
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
	};
	if let Some(style) = cursor {
		draw_cursor(&mut layer, grid.cursor_row, grid.cursor_col, style, highlights);
	}
	layer
}

/// Draws a block cursor by giving the cells under it the cursor's colors, so the character stays readable, and
/// a bar cursor as the layer's [`BarCursor`]. The colors come from the cursor's highlight, or are the cell's
/// swapped when it has none (`:help ui-event-mode_info_set`).
fn draw_cursor(layer: &mut CellLayer, row: usize, col: usize, style: CursorStyle, highlights: &HighlightState) {
	let width = layer.width as usize;
	let index = row * width + col;
	let Some(&cell) = layer.cells.get(index).filter(|_| col < width) else { return };

	// A double-width character also covers the next cell, which is empty.
	let double_width = col + 1 < width && layer.cells[index + 1].text.is_empty();
	let covered = if double_width { 2 } else { 1 };

	let (foreground, background) = if style.attr_id == 0 {
		(cell.background, cell.style.foreground)
	} else {
		let colors = highlights.resolve(style.attr_id);
		(colors.foreground, colors.background)
	};

	let orientation = match style.shape {
		CursorShape::Block => {
			for cell in &mut layer.cells[index..index + covered] {
				cell.style.foreground = foreground;
				cell.background = background;
			}
			return;
		},
		CursorShape::Vertical => BarOrientation::Vertical,
		CursorShape::Horizontal => BarOrientation::Horizontal,
	};

	layer.cursor = Some(BarCursor {
		row: row as u32,
		col: col as u32,
		width: covered as u32,
		orientation,
		percentage: style.percentage,
		color: background,
	});
}

#[cfg(test)]
mod tests {
	use nvim::{CellText, CursorShape, DefaultColorsSetEvent, GridCell, HlAttrDefineEvent, HlAttrs};
	use renderer::{BarCursor, BarOrientation, TextStyle};

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

	#[test]
	fn block_cursor_swaps_the_cell_colors() {
		let layer = cell_layer(1, &grid_with_cursor(1), &highlights(), Some(style(CursorShape::Block, 0, 0)));
		// "b" is 0x112233 on the default background, so the cursor is the default background on 0x112233.
		assert_eq!(colors(&layer), [(FG, BG), (BG, 0x112233), (FG, BG), (FG, BG)]);
		assert_eq!(layer.cursor, None);
	}

	#[test]
	fn block_cursor_uses_its_highlight() {
		let layer = cell_layer(1, &grid_with_cursor(0), &highlights(), Some(style(CursorShape::Block, 0, 3)));
		assert_eq!(colors(&layer)[0], (CURSOR_FG, CURSOR_BG));
	}

	#[test]
	fn block_cursor_covers_both_halves_of_a_double_width_character() {
		let layer = cell_layer(1, &grid_with_cursor(2), &highlights(), Some(style(CursorShape::Block, 0, 3)));
		assert_eq!(colors(&layer), [(FG, BG), (0x112233, BG), (CURSOR_FG, CURSOR_BG), (CURSOR_FG, CURSOR_BG)]);
	}

	#[test]
	fn bar_cursors() {
		let layer = cell_layer(1, &grid_with_cursor(1), &highlights(), Some(style(CursorShape::Vertical, 25, 0)));
		let bar = |orientation, col, width, color| BarCursor { row: 0, col, width, orientation, percentage: 25, color };
		// The cell keeps its colors; the bar takes the swapped background, i.e. the cell's text color.
		assert_eq!(colors(&layer)[1], (0x112233, BG));
		assert_eq!(layer.cursor, Some(bar(BarOrientation::Vertical, 1, 1, 0x112233)));

		let layer = cell_layer(1, &grid_with_cursor(2), &highlights(), Some(style(CursorShape::Horizontal, 25, 3)));
		assert_eq!(layer.cursor, Some(bar(BarOrientation::Horizontal, 2, 2, CURSOR_BG)));
	}

	#[test]
	fn no_cursor_when_hidden_or_outside_the_grid() {
		let plain = cell_layer(1, &grid_with_cursor(1), &highlights(), None);
		assert_eq!(colors(&plain), [(FG, BG), (0x112233, BG), (FG, BG), (FG, BG)]);

		let mut grid = grid_with_cursor(0);
		grid.cursor_col = 7;
		let layer = cell_layer(1, &grid, &highlights(), Some(style(CursorShape::Block, 0, 0)));
		assert_eq!(colors(&layer), colors(&plain));
	}
}
