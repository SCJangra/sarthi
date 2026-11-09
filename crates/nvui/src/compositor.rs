use renderer::{CellLayer, RenderCell, TextStyle};

use crate::grid::Grid;
use crate::highlight::HighlightState;

/// A snapshot of grid `id` for the renderer, with every cell's highlight resolved. Without multigrid,
/// Neovim draws everything to grid 1, which fills the window.
pub fn cell_layer(id: u32, grid: &Grid, highlights: &HighlightState) -> CellLayer {
	let cells = grid
		.cells
		.iter()
		.map(|cell| {
			let style = highlights.resolve(cell.hl_id);
			RenderCell {
				text: cell.text,
				style: TextStyle { foreground: style.foreground, bold: style.bold, italic: style.italic },
			}
		})
		.collect();

	CellLayer { id: u64::from(id), row: 0, col: 0, width: grid.width as u32, height: grid.height as u32, cells }
}

#[cfg(test)]
mod tests {
	use nvim::{CellText, DefaultColorsSetEvent, GridCell, HlAttrDefineEvent, HlAttrs};
	use renderer::TextStyle;

	use super::cell_layer;
	use crate::grid::Grid;
	use crate::highlight::HighlightState;
	use crate::highlight::tests::attrs;

	const FG: u32 = 0xC7C7C7;
	const BG: u32 = 0x1C1C1C;

	fn highlights() -> HighlightState {
		let mut state = HighlightState::new();
		let colors = DefaultColorsSetEvent { rgb_fg: FG, rgb_bg: BG, rgb_sp: 0xFF0000, cterm_fg: 0, cterm_bg: 0 };
		state.set_default_colors(vec![colors]);

		let bold = HlAttrs { foreground: Some(0x112233), bold: true, ..attrs() };
		let reverse = HlAttrs { reverse: true, italic: true, ..attrs() };
		state.set_attrs(vec![
			HlAttrDefineEvent { id: 1, rgb_attr: bold.clone(), cterm_attr: bold, info: vec![] },
			HlAttrDefineEvent { id: 2, rgb_attr: reverse.clone(), cterm_attr: reverse, info: vec![] },
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

		let layer = cell_layer(1, &grid, &highlights());
		assert_eq!((layer.id, layer.row, layer.col, layer.width, layer.height), (1, 0, 0, 3, 2));
		assert_eq!(layer.cells.len(), 6);

		let plain = TextStyle { foreground: FG, bold: false, italic: false };
		assert_eq!(layer.cells[0].text.as_str(), " ");
		assert_eq!(layer.cells[0].style, plain);

		let row: Vec<_> = layer.cells[3..].iter().map(|cell| (cell.text.as_str(), cell.style)).collect();
		assert_eq!(
			row,
			[
				("a", plain),
				("b", TextStyle { foreground: 0x112233, bold: true, italic: false }),
				// Reverse: the default background becomes the text color.
				("c", TextStyle { foreground: BG, bold: false, italic: true }),
			],
		);
	}
}
