//! Neovim's UI state, from its redraw events, and the layers the renderer draws from it. Knows nothing about windows.

pub mod compositor;
pub mod cursor;
pub mod grid;
pub mod highlight;

use std::collections::HashMap;

use nvim::RedrawNotification;
use renderer::Layer;

use crate::compositor::cell_layer;
use crate::cursor::CursorState;
use crate::grid::Grid;
use crate::highlight::HighlightState;

/// Neovim's UI state, kept up to date from its redraw events (`:help ui-events`)
#[derive(Debug)]
pub struct Editor {
	/// Neovim's grids by id. Without multigrid, Neovim draws everything to grid 1.
	grids: HashMap<u32, Grid>,
	highlights: HighlightState,
	cursor: CursorState,

	/// The grid the cursor is in
	cursor_grid: u32,

	/// Whether anything changed since the last `flush`, so it's drawn on the next one
	stale: bool,
}

/// What to draw after Neovim finished redrawing
#[derive(Debug, Clone, PartialEq)]
pub struct Redraw {
	/// Neovim's default background, as `0xRRGGBB`, for the parts of a window no grid covers
	pub background: u32,

	/// Drawn in order, so later ones cover earlier ones
	pub layers: Vec<Layer>,
}

impl Default for Editor {
	fn default() -> Self {
		Self::new()
	}
}

impl Editor {
	pub fn new() -> Self {
		Self {
			grids: HashMap::from([(1, Grid::new(0, 0))]),
			highlights: HighlightState::new(),
			cursor: CursorState::default(),
			cursor_grid: 1,
			stale: false,
		}
	}

	/// Neovim's default background, as `0xRRGGBB`
	pub fn background(&self) -> u32 {
		self.highlights.defaults().background
	}

	/// Applies a batch of redraw events in order. Neovim's state is only consistent once a redraw ends (`flush`), so
	/// that's when this returns what to draw, if anything changed.
	pub fn apply(&mut self, events: Vec<RedrawNotification>) -> Option<Redraw> {
		let mut redraw = None;
		for event in events {
			redraw = self.apply_event(event).or(redraw);
		}
		redraw
	}

	fn apply_event(&mut self, event: RedrawNotification) -> Option<Redraw> {
		match event {
			RedrawNotification::GridResize(resizes) => {
				for resize in resizes {
					self.update_grid(resize.grid, "grid_resize", |grid| {
						grid.resize(resize.width as usize, resize.height as usize);
					});
				}
			},
			RedrawNotification::GridClear(clears) => {
				for clear in clears {
					self.update_grid(clear.grid, "grid_clear", Grid::clear);
				}
			},
			RedrawNotification::GridLine(lines) => {
				for line in lines {
					self.update_grid(line.grid, "grid_line", |grid| {
						grid.set_line(line.row as usize, line.col_start as usize, &line.cells);
					});
				}
			},
			RedrawNotification::GridScroll(scrolls) => {
				for scroll in scrolls {
					self.update_grid(scroll.grid, "grid_scroll", |grid| {
						let (top, bot) = (scroll.top as usize, scroll.bot as usize);
						grid.scroll(top, bot, scroll.left as usize, scroll.right as usize, scroll.rows as isize);
					});
				}
			},
			RedrawNotification::GridCursorGoto(gotos) => {
				for goto in gotos {
					self.update_grid(goto.grid, "grid_cursor_goto", |grid| {
						grid.set_cursor(goto.row as usize, goto.col as usize);
					});
					self.cursor_grid = goto.grid;
				}
			},
			RedrawNotification::HlAttrDefine(attrs) => {
				self.highlights.set_attrs(attrs);
				self.stale = true;
			},
			RedrawNotification::DefaultColorsSet(colors) => {
				self.highlights.set_default_colors(colors);
				self.stale = true;
			},
			RedrawNotification::HlGroupSet(groups) => self.highlights.set_hl_groups(groups),
			RedrawNotification::ModeInfoSet(sets) => {
				if let Some(set) = sets.into_iter().last() {
					self.cursor.set_modes(set.cursor_style_enabled, set.mode_info);
					self.stale = true;
				}
			},
			RedrawNotification::ModeChange(changes) => {
				if let Some(change) = changes.last() {
					self.cursor.set_mode(change.mode_idx as usize);
					self.stale = true;
				}
			},
			RedrawNotification::BusyStart => {
				self.cursor.set_hidden(true);
				self.stale = true;
			},
			RedrawNotification::BusyStop => {
				self.cursor.set_hidden(false);
				self.stale = true;
			},
			RedrawNotification::Flush => return self.flush(),
			_ => (),
		}
		None
	}

	/// Applies `change` to grid `id`, to be drawn on the next `flush`. `event` names the event for the warning about
	/// an unknown grid.
	fn update_grid(&mut self, id: u32, event: &str, change: impl FnOnce(&mut Grid)) {
		let Some(grid) = self.grids.get_mut(&id) else {
			tracing::warn!("{event}: unknown grid {id}");
			return;
		};
		change(grid);
		self.stale = true;
	}

	/// The layers to draw, if anything changed since the last `flush`: one per grid, by id
	fn flush(&mut self) -> Option<Redraw> {
		if !std::mem::take(&mut self.stale) {
			return None;
		}

		let mut ids: Vec<u32> = self.grids.keys().copied().collect();
		ids.sort_unstable();
		let layers = ids
			.into_iter()
			.map(|id| {
				let cursor = (id == self.cursor_grid).then(|| self.cursor.style()).flatten();
				Layer::Cells(cell_layer(id, &self.grids[&id], &self.highlights, cursor))
			})
			.collect();

		Some(Redraw { background: self.background(), layers })
	}
}

#[cfg(test)]
mod tests {
	use nvim::{
		CellText, DefaultColorsSetEvent, GridCell, GridCursorGotoEvent, GridLineEvent, GridResizeEvent,
		RedrawNotification,
	};
	use renderer::Layer;

	use super::{Editor, Redraw};

	const FG: u32 = 0xC7C7C7;
	const BG: u32 = 0x1C1C1C;

	fn resize(grid: u32, width: u32, height: u32) -> RedrawNotification {
		RedrawNotification::GridResize(vec![GridResizeEvent { grid, width, height }])
	}

	fn line(grid: u32, row: u32, text: &str) -> RedrawNotification {
		let cells = text
			.chars()
			.map(|c| GridCell { text: CellText::from(&c.to_string()).unwrap(), hl_id: None, repeat: None })
			.collect();
		RedrawNotification::GridLine(vec![GridLineEvent { grid, row, col_start: 0, cells, wrap: false }])
	}

	fn goto(grid: u32, row: u32, col: u32) -> RedrawNotification {
		RedrawNotification::GridCursorGoto(vec![GridCursorGotoEvent { grid, row, col }])
	}

	fn colors() -> RedrawNotification {
		RedrawNotification::DefaultColorsSet(vec![DefaultColorsSetEvent {
			rgb_fg: FG,
			rgb_bg: BG,
			rgb_sp: 0xFF0000,
			cterm_fg: 0,
			cterm_bg: 0,
		}])
	}

	/// The text of each row of the only layer of `redraw`
	fn rows(redraw: &Redraw) -> Vec<String> {
		let [Layer::Cells(layer)] = redraw.layers.as_slice() else { panic!("expected one layer") };
		layer
			.cells
			.chunks(layer.width as usize)
			.map(|row| row.iter().map(|cell| cell.text.as_str()).collect())
			.collect()
	}

	#[test]
	fn draws_only_when_a_redraw_ends() {
		let mut editor = Editor::new();
		assert_eq!(editor.apply(vec![colors(), resize(1, 3, 2), line(1, 0, "abc")]), None);

		let redraw = editor.apply(vec![line(1, 1, "def"), RedrawNotification::Flush]).unwrap();
		assert_eq!(redraw.background, BG);
		assert_eq!(rows(&redraw), ["abc", "def"]);
	}

	#[test]
	fn nothing_to_draw_without_changes() {
		let mut editor = Editor::new();
		assert!(editor.apply(vec![resize(1, 2, 1), RedrawNotification::Flush]).is_some());
		assert_eq!(editor.apply(vec![RedrawNotification::Flush]), None);
		// An unknown grid changes nothing.
		assert_eq!(editor.apply(vec![line(7, 0, "x"), RedrawNotification::Flush]), None);
	}

	#[test]
	fn the_last_redraw_of_a_batch_wins() {
		let mut editor = Editor::new();
		let events = vec![
			resize(1, 1, 1),
			line(1, 0, "a"),
			RedrawNotification::Flush,
			line(1, 0, "b"),
			RedrawNotification::Flush,
		];
		assert_eq!(rows(&editor.apply(events).unwrap()), ["b"]);
	}

	#[test]
	fn cursor_is_drawn_where_neovim_put_it() {
		let mut editor = Editor::new();
		let events = vec![colors(), resize(1, 2, 1), line(1, 0, "ab"), goto(1, 0, 1), RedrawNotification::Flush];
		let redraw = editor.apply(events).unwrap();
		let [Layer::Cells(layer)] = redraw.layers.as_slice() else { panic!("expected one layer") };
		// A block cursor in the cell's swapped colors, on "b" only
		let colors: Vec<_> = layer
			.cells
			.iter()
			.map(|cell| (cell.style.foreground, cell.background))
			.collect();
		assert_eq!(colors, [(FG, BG), (BG, FG)]);

		// Moving the cursor is a change to draw.
		assert!(editor.apply(vec![goto(1, 0, 0), RedrawNotification::Flush]).is_some());
	}
}
