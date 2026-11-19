//! The text an input method is composing (its preedit), shown at the cursor until it's committed to Neovim

use renderer::{BarOrientation, Cursor, CursorShape, Layer, Rect, RenderCell, Underline};
use rpc::neovim::CellText;
use unicode_width::UnicodeWidthChar;

/// An id no grid has: Neovim's grid ids are 32-bit, and the message separator takes the one after them.
const PREEDIT_ID: u64 = u32::MAX as u64 + 2;

/// Text an input method is composing, and where its caret is in it, as a byte offset
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preedit {
	pub text: String,
	pub caret: Option<usize>,
}

/// The cursor among `layers`, the last one's if more have one, in pixels. Not the caret in the
/// text an input method composes: its candidates stay by the cursor, rather than move with each key.
pub fn cursor_rect(layers: &[Layer]) -> Option<Rect<u32>> {
	layers.iter().rev().find_map(|cells| {
		let cursor = cells.cursor.filter(|_| cells.id != PREEDIT_ID)?;
		let (width, height) = (cells.cell.width, cells.cell.height);
		Some(Rect::new(cells.x + cursor.col * width, cells.y + cursor.row * height, cursor.width * width, height))
	})
}

/// A layer showing `preedit` over the cursor's cell and those after it, underlined, in the colors of the cell under the
/// cursor. The cursor moves into it, to the preedit's caret, or is hidden without one. `None` without a cursor.
pub fn preedit_layer(layers: &mut [Layer], preedit: &Preedit) -> Option<Layer> {
	let cursor_layer = layers.iter_mut().rev().find(|layer| layer.cursor.is_some())?;
	let cursor = cursor_layer.cursor.take()?;
	let under = cursor_layer.cells.get(cursor_layer.index(cursor.row, cursor.col)).copied()?;
	let style = RenderCell { blend: 0, ..under };
	let mut decoration = style.decoration;
	decoration.underline = Some(Underline::Single);
	decoration.underline_color = style.style.foreground;

	let mut cells = Vec::new();
	let mut caret_col = None;
	for (offset, character) in preedit.text.char_indices() {
		if preedit.caret == Some(offset) {
			caret_col = Some(cells.len() as u32);
		}
		match character.width() {
			// Combining characters join the cell before them.
			Some(0) | None => {
				if let Some(last) = cells.iter_mut().rev().find(|cell: &&mut RenderCell| !cell.text.is_empty()) {
					let _ = last.text.try_push(character);
				}
			},
			Some(width) => {
				let text = CellText::from(character.encode_utf8(&mut [0; 4])).ok()?;
				cells.push(RenderCell { text, decoration, ..style });
				// The right half of a double-width character
				for _ in 1..width {
					cells.push(RenderCell { text: CellText::new(), decoration, ..style });
				}
			},
		}
	}
	if preedit.caret == Some(preedit.text.len()) {
		caret_col = Some(cells.len() as u32);
	}
	if cells.is_empty() {
		cursor_layer.cursor = Some(cursor);
		return None;
	}
	// Room for a caret after the text
	let width = cells.len() as u32 + u32::from(caret_col == Some(cells.len() as u32));
	cells.resize(width as usize, RenderCell { text: CellText::from(" ").ok()?, ..style });

	let (cell, x, y) = (cursor_layer.cell, cursor_layer.x, cursor_layer.y);
	let caret = caret_col.map(|col| Cursor {
		row: 0,
		col,
		width: 1,
		shape: CursorShape::Bar { orientation: BarOrientation::Vertical, percentage: bar_percentage(cursor.shape) },
		color: cursor.color,
		text_color: cursor.text_color,
		animate: false,
	});
	Some(Layer {
		id: PREEDIT_ID,
		x: x + cursor.col * cell.width,
		y: y + cursor.row * cell.height,
		font: cursor_layer.font,
		cell,
		width,
		height: 1,
		cells,
		cursor: caret,
		scroll: None,
		shadow: None,
		outline: None,
		padding: renderer::Edges::default(),
		padding_background: (style.background, 0),
		corner_radius: 0.0,
	})
}

/// How much of a cell the caret covers: as much as the cursor's own vertical bar, as in insert mode, otherwise a quarter
fn bar_percentage(shape: CursorShape) -> u32 {
	match shape {
		CursorShape::Bar { orientation: BarOrientation::Vertical, percentage } => percentage,
		_ => 25,
	}
}

#[cfg(test)]
mod tests {
	use renderer::{
		BarOrientation, CellSize, Cursor, CursorShape, Decoration, FontId, Layer, RenderCell, TextStyle, Underline,
	};
	use rpc::neovim::CellText;

	use super::{Preedit, cursor_rect, preedit_layer};

	const CELL: CellSize = CellSize { width: 10, height: 20 };
	const FG: u32 = 0x112233;
	const BG: u32 = 0x445566;

	/// A 6×2 layer at (100, 40), its cursor at row 1, column 2, as an insert mode bar
	fn layer() -> Layer {
		let cell = RenderCell {
			text: CellText::from("x").unwrap(),
			style: TextStyle { foreground: FG, bold: false, italic: false },
			background: BG,
			blend: 100,
			decoration: Decoration::default(),
		};
		let shape = CursorShape::Bar { orientation: BarOrientation::Vertical, percentage: 15 };
		Layer {
			id: 1,
			x: 100,
			y: 40,
			font: FontId::MAIN,
			cell: CELL,
			width: 6,
			height: 2,
			cells: vec![cell; 12],
			cursor: Some(Cursor { row: 1, col: 2, width: 1, shape, color: 0xFF0000, text_color: 0, animate: true }),
			scroll: None,
			shadow: None,
			outline: None,
			padding: renderer::Edges::default(),
			padding_background: (BG, 0),
			corner_radius: 0.0,
		}
	}

	fn preedit(text: &str, caret: Option<usize>) -> Preedit {
		Preedit { text: text.to_owned(), caret }
	}

	#[test]
	fn cursor_rect_is_the_cursors_cell_in_pixels() {
		assert_eq!(cursor_rect(&[layer()]), Some(renderer::Rect::new(120, 60, 10, 20)));
		// While text is composed, the cursor is its caret, which doesn't count.
		let mut layers = vec![layer()];
		let shown = preedit_layer(&mut layers, &preedit("ab", Some(1))).unwrap();
		layers.push(shown);
		assert_eq!(cursor_rect(&layers), None);
	}

	#[test]
	fn preedit_covers_the_cells_from_the_cursor_on_underlined() {
		let mut layers = [layer()];
		let shown = preedit_layer(&mut layers, &preedit("ab", None)).unwrap();
		assert_eq!((shown.x, shown.y, shown.width, shown.height), (120, 60, 2, 1));
		let texts: Vec<&str> = shown.cells.iter().map(|cell| cell.text.as_str()).collect();
		assert_eq!(texts, ["a", "b"]);
		let first = shown.cells[0];
		assert_eq!((first.background, first.blend, first.style.foreground), (BG, 0, FG));
		assert_eq!(first.decoration.underline, Some(Underline::Single));
		// No caret: the cursor is hidden, rather than drawn under the text.
		assert_eq!(shown.cursor, None);
		assert_eq!(layers[0].cursor, None);
	}

	#[test]
	fn double_width_characters_take_two_cells() {
		let mut layers = [layer()];
		let shown = preedit_layer(&mut layers, &preedit("日本", None)).unwrap();
		let texts: Vec<&str> = shown.cells.iter().map(|cell| cell.text.as_str()).collect();
		assert_eq!(texts, ["日", "", "本", ""]);
	}

	#[test]
	fn the_cursor_moves_to_the_caret() {
		let mut layers = [layer()];
		// After "日", 3 bytes in: its two cells
		let shown = preedit_layer(&mut layers, &preedit("日本", Some(3))).unwrap();
		let caret = shown.cursor.unwrap();
		assert_eq!((caret.col, caret.color, caret.animate), (2, 0xFF0000, false));
		assert_eq!(caret.shape, CursorShape::Bar { orientation: BarOrientation::Vertical, percentage: 15 });
		// At the end, a cell after the text holds it.
		let shown = preedit_layer(&mut [layer()], &preedit("ab", Some(2))).unwrap();
		assert_eq!((shown.width, shown.cursor.map(|caret| caret.col)), (3, Some(2)));
	}

	#[test]
	fn combining_characters_join_the_cell_before_them() {
		let shown = preedit_layer(&mut [layer()], &preedit("e\u{301}", None)).unwrap();
		let texts: Vec<&str> = shown.cells.iter().map(|cell| cell.text.as_str()).collect();
		assert_eq!(texts, ["e\u{301}"]);
	}

	#[test]
	fn nothing_without_a_cursor() {
		let mut layers = [layer()];
		layers[0].cursor = None;
		assert_eq!(preedit_layer(&mut layers, &preedit("ab", None)), None);
	}
}
