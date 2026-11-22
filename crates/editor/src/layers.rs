//! The layers to draw: each shown grid as a layer, stacked as Neovim's own compositor would, with their frames,
//! shadows and the text an input method composes

use renderer::{Decoration, Edges, Layer, LayerShadow, Rect, RenderCell, Scroll};
use rpc::neovim::{CellText, Grid, GridCell as NvimGridCell, Window};

use crate::compositor::cell_layer;
use crate::grid::GridState;
use crate::preedit::preedit_layer;
use crate::{Editor, Placement};

impl Editor {
	/// The grids shown over grid 1, in Neovim's drawing order, like its own compositor (`:help
	/// ui-event-win_float_pos`): the windows, then floating windows and the message grid by `compindex`; and over all
	/// of them, the external windows in their order. Grids that aren't placed aren't shown.
	pub(crate) fn stacked(&self) -> Vec<(Grid, &Placement)> {
		// Sort keys: tiled windows, floats and messages, then external windows; within each, by drawing order or grid id
		let mut placed: Vec<(Grid, &Placement)> =
			self.placements.iter().map(|(&id, placement)| (id, placement)).collect();
		placed.retain(|(id, placement)| *placement != &Placement::Hidden && self.grids.contains_key(id));
		placed.sort_by_key(|&(id, placement)| match placement {
			Placement::Tiled { .. } | Placement::Hidden => (0, 0, id),
			Placement::Float { compindex, .. } | Placement::Message { compindex, .. } => (1, u64::from(*compindex), id),
			Placement::External { order, .. } => (2, *order, id),
		});
		placed
	}

	/// The layers in drawing order: grid 1, then the grids shown over it (see [`Editor::stacked`])
	pub(crate) fn layers(&self) -> Vec<Layer> {
		let screen_height = self.grids.get(&Grid::GLOBAL).map_or(0, |grid| grid.height);
		let placed = self.stacked();

		let mut shadows = self.float_shadows(&placed);
		shadows.extend(self.external_shadows(&placed));
		// What covers grid 1, and what covers the tiled windows, in cells
		let screen_width = self.grids.get(&Grid::GLOBAL).map_or(0, |grid| grid.width);
		let area_of = |kind: fn(&Placement) -> Option<[u32; 2]>| -> Vec<Rect<u32>> {
			placed
				.iter()
				.filter_map(|(id, placement)| Some((self.grids.get(id)?, kind(placement)?)))
				.map(|(grid, [col, row])| Rect::new(col, row, grid.width, grid.height))
				.collect()
		};
		let tiled = area_of(|placement| match placement {
			Placement::Tiled { row, col } => Some([*col, *row]),
			_ => None,
		});
		let messages: Vec<Rect<u32>> = placed
			.iter()
			.filter_map(|(_, placement)| match placement {
				Placement::Message { row, .. } => {
					Some(Rect::new(0, *row, screen_width, screen_height.saturating_sub(*row)))
				},
				_ => None,
			})
			.collect();
		let under_windows: Vec<Rect<u32>> = tiled.iter().chain(&messages).copied().collect();

		let mut layers = Vec::with_capacity(placed.len() + 2);
		if self.grids.contains_key(&Grid::GLOBAL) {
			layers.push(self.see_through_cells(self.layer(Grid::GLOBAL, 0, 0), (0, 0), &under_windows));
		}
		for (id, placement) in placed {
			match placement {
				Placement::Tiled { row, col } => {
					layers.push(self.see_through_cells(self.layer(id, *row, *col), (*row, *col), &messages))
				},
				// Placed by Neovim in its cells: a shadow, but no frame (outline, corners, padding), which Neovim
				// wouldn't leave room for
				Placement::Float { row, col, .. } => {
					let mut layer = self.layer(id, *row, *col);
					layer.shadow = shadows.remove(&id);
					layers.push(layer);
				},
				Placement::Message { row, separator, .. } => {
					if let Some(separator) = separator {
						layers.push(self.separator(*row - 1, *separator));
					}
					let mut layer = self.layer(id, *row, 0);
					// The message grid is as tall as the screen; only its rows from `row` down show.
					clip_rows(&mut layer, screen_height.saturating_sub(*row));
					layers.push(self.see_through_cells(layer, (*row, 0), &[]));
				},
				Placement::External { win, .. } => layers.push(self.external_layer(id, *win, shadows.remove(&id))),
				Placement::Hidden => (),
			}
		}
		if let Some(preedit) = &self.preedit
			&& let Some(layer) = preedit_layer(&mut layers, preedit)
		{
			layers.push(layer);
		}
		layers
	}

	/// `layer`, a tiled window, grid 1 or the message grid, with cells in the default background drawing no background
	/// of their own while the window is see-through, so what's behind the window shows. Floating windows keep theirs,
	/// or what's under them would show.
	///
	/// Its cells under `covered` (in cells of the screen), what other grids show over it, are
	/// left blank then: like in Neovim's own compositor, a screen cell shows only its top grid. Neovim leaves old
	/// separators and status lines in grid 1 under the windows, which opaque windows hide. The layer is at `origin`, a
	/// row and column of the screen.
	fn see_through_cells(&self, mut layer: Layer, origin: (u32, u32), covered: &[Rect<u32>]) -> Layer {
		if !self.see_through {
			return layer;
		}
		let background = self.background();
		for cell in layer.cells.iter_mut().filter(|cell| cell.background == background) {
			cell.blend = 100;
		}

		let blank = CellText::from(" ").expect("a space fits in a cell");
		let (width, height, (origin_row, origin_col)) = (layer.width, layer.height, origin);
		for cover in covered {
			// In the layer's cells
			let (left, top) = (cover.x.saturating_sub(origin_col).min(width), cover.y.saturating_sub(origin_row));
			let right = cover.right().saturating_sub(origin_col).clamp(left, width);
			let bottom = cover.bottom().saturating_sub(origin_row).min(height);
			for layer_row in top..bottom {
				let start = layer.index(layer_row, 0);
				for cell in &mut layer.cells[start + left as usize..start + right as usize] {
					*cell = RenderCell { text: blank, blend: 100, decoration: Decoration::default(), ..*cell };
				}
			}
		}
		layer
	}

	/// Grid `id` as a layer at `row`, `col` of the screen, with the cursor if it's in this grid
	fn layer(&self, id: Grid, row: u32, col: u32) -> Layer {
		let cursor = (id == self.cursor_grid).then(|| self.cursor.style()).flatten();
		let (font, cell) = self.font_of(id);
		let mut layer = cell_layer(&self.grids[&id], &self.highlights, cursor, cell);
		// The grid's handle identifies its layer.
		(layer.id, layer.font) = (u64::from(id.0), font);
		(layer.x, layer.y) = (col * self.cell.width, row * self.cell.height);
		layer.scroll = self.scroll(id, layer.width, layer.height);
		layer
	}

	/// External window `win`, grid `id`, as a layer where it's placed (see [`Editor::set_external_place`]), with
	/// `shadow`, the shadow drawn with it (see [`Editor::external_shadows`])
	fn external_layer(&self, id: Grid, win: Window, shadow: Option<LayerShadow>) -> Layer {
		let mut layer = self.layer(id, 0, 0);
		let rect = self.external_rect(id, win);
		(layer.x, layer.y) = (rect.x, rect.y);
		layer.shadow = shadow;
		layer.outline = self.outline_of(win);
		layer.corner_radius = self.corners_of(win);
		layer.padding = self.padding_of(win);
		let style = self.style_of(win);
		layer.padding_background = (style.background.unwrap_or_else(|| self.background()), style.blend);
		layer
	}

	/// How the text of grid `id`, `width` × `height` cells, scrolled since the last `flush`, if it did
	fn scroll(&self, id: Grid, width: u32, height: u32) -> Option<Scroll> {
		let viewport = self.viewports.get(&id).filter(|viewport| viewport.scrolled != 0)?;
		let animation = viewport.win.and_then(|win| self.scroll_animations.get(&win));
		// It jumps.
		if animation.is_some_and(Option::is_none) {
			return None;
		}
		let lines = viewport.scrolled.unsigned_abs().try_into().unwrap_or(u32::MAX);
		let motion = animation
			.and_then(Option::as_ref)
			.map(|animation| crate::motion(animation.curve_for(lines)));
		let Edges { top, right, bottom, left } = viewport.margins;
		Some(Scroll {
			rows: viewport.scrolled.clamp(i32::MIN.into(), i32::MAX.into()) as i32,
			top,
			bottom: height.saturating_sub(bottom),
			left,
			right: width.saturating_sub(right),
			motion,
		})
	}

	/// The line the TUI draws above messages that scrolled over the windows: `separator` across the screen at `row`,
	/// in the `MsgSeparator` highlight
	fn separator(&self, row: u32, separator: CellText) -> Layer {
		let width = self.grids.get(&Grid::GLOBAL).map_or(0, |grid| grid.width);
		let mut line = GridState::new(width, 1);
		let hl_id = Some(self.highlights.group("MsgSeparator"));
		line.set_line(0, 0, &[NvimGridCell { text: separator, hl_id, repeat: Some(width) }]);

		let mut layer = cell_layer(&line, &self.highlights, None, self.cell);
		// An id no grid has: Neovim's grid ids are 32-bit.
		(layer.id, layer.y) = (u64::from(u32::MAX) + 1, row * self.cell.height);
		layer
	}
}

/// Keeps the first `height` rows of `layer`, if it's taller
fn clip_rows(layer: &mut Layer, height: u32) {
	let Layer { height: layer_height, width, cells, cursor, .. } = layer;
	if *layer_height <= height {
		return;
	}
	*layer_height = height;
	cells.truncate((height * *width) as usize);
	if cursor.is_some_and(|cursor| cursor.row >= height) {
		*cursor = None;
	}
}
