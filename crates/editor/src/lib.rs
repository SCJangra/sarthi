//! Neovim's UI state, from its redraw events, and the layers the renderer draws from it. Knows nothing about windows.

pub mod compositor;
pub mod cursor;
pub mod grid;
pub mod highlight;

use std::collections::HashMap;

use nvim::{CellText, GridCell as NvimGridCell, OptionValue, RedrawNotification};
use renderer::{CellLayer, Layer, Scroll};

use crate::compositor::cell_layer;
use crate::cursor::CursorState;
use crate::grid::Grid;
use crate::highlight::HighlightState;

/// Neovim's UI state, kept up to date from its redraw events (`:help ui-events`)
#[derive(Debug)]
pub struct Editor {
	/// Neovim's grids by id (`:help ui-multigrid`). Grid 1 is the screen: it holds what isn't in a window, like
	/// window separators, status lines and the command line, and the windows' grids are placed on it.
	grids: HashMap<u32, Grid>,

	/// Where each grid other than grid 1 is shown; grids without one aren't shown yet
	placements: HashMap<u32, Placement>,

	/// How each window's text scrolls, by grid
	viewports: HashMap<u32, Viewport>,

	highlights: HighlightState,
	cursor: CursorState,

	/// The grid the cursor is in
	cursor_grid: u32,

	/// The `guifont` option, as Neovim sent it, or empty while it's Neovim's default
	guifont: String,

	/// Neovim's default `guifont`, the first value it sent
	default_guifont: Option<String>,

	/// The `linespace` option: pixels added between rows, or taken away if negative
	linespace: i64,

	/// Whether mouse input is on in the current mode (`mouse_on`, `mouse_off`)
	mouse: bool,

	/// Whether Neovim wants mouse moves without a button held (the `mousemoveevent` option)
	mouse_move_events: bool,

	/// Whether anything changed since the last `flush`, so it's drawn on the next one
	stale: bool,
}

/// Where a grid is shown, from Neovim's multigrid events
#[derive(Debug, Clone, PartialEq)]
enum Placement {
	/// A window at a cell of grid 1 (`win_pos`)
	Tiled { row: u32, col: u32 },

	/// A floating window at a cell of the screen, in Neovim's drawing order of floats (`win_float_pos`). Without
	/// `mouse`, clicks go to what's below it.
	Float { row: u32, col: u32, compindex: u32, mouse: bool },

	/// The message grid, across the screen from `row` down (`msg_set_pos`). When messages scrolled over the windows,
	/// a line of `separator` goes above them.
	Message { row: u32, compindex: u32, separator: Option<CellText> },

	/// Not shown, e.g. a window of another tab (`win_hide`)
	Hidden,
}

/// How a window's text scrolls, from `win_viewport` and `win_viewport_margins`
#[derive(Debug, Default)]
struct Viewport {
	/// Lines in the window's buffer. When it changes, the window likely shows another buffer, which Neovim also
	/// reports as a scroll.
	line_count: u32,

	/// Rows and columns at the window's edges that don't scroll, like the winbar or a border: top, bottom, left, right
	margins: [u32; 4],

	/// Rows the text moved up since the last `flush`, or down if negative
	scrolled: i64,
}

/// A cell of a grid
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPosition {
	pub grid: u32,
	pub row: u32,
	pub col: u32,
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
			grids: HashMap::new(),
			placements: HashMap::new(),
			viewports: HashMap::new(),
			highlights: HighlightState::new(),
			cursor: CursorState::default(),
			cursor_grid: 1,
			guifont: String::new(),
			default_guifont: None,
			linespace: 0,
			mouse: false,
			mouse_move_events: false,
			stale: false,
		}
	}

	/// Neovim's default background, as `0xRRGGBB`
	pub fn background(&self) -> u32 {
		self.highlights.defaults().background
	}

	/// The `guifont` option (`:help 'guifont'`): which fonts to draw text with. Empty unless set to something other than
	/// Neovim's default (a list of common fonts, while nvui has its own default font).
	pub fn guifont(&self) -> &str {
		&self.guifont
	}

	/// The `linespace` option (`:help 'linespace'`): logical pixels added between rows, or taken away if negative
	pub fn linespace(&self) -> i64 {
		self.linespace
	}

	/// Whether mouse input is on in the current mode, as the `mouse` option says (`mouse_on`, `mouse_off`)
	pub fn mouse(&self) -> bool {
		self.mouse
	}

	/// Whether Neovim wants mouse moves without a button held (the `mousemoveevent` option)
	pub fn mouse_move_events(&self) -> bool {
		self.mouse_move_events
	}

	/// The grid shown at cell `row`, `col` of the screen, and that cell in it: the topmost grid there that takes the
	/// mouse, so not a float made with `mouse = false` (`:help ui-multigrid`)
	pub fn grid_at(&self, row: u32, col: u32) -> Option<GridPosition> {
		let screen_height = self.grids.get(&1).map_or(0, |grid| grid.height as u32);
		let covers = |id: u32, top: u32, left: u32, bottom: u32| {
			let grid = &self.grids[&id];
			let right = left + grid.width as u32;
			(top..bottom).contains(&row) && (left..right).contains(&col)
		};

		let over = self.stacked().into_iter().rev().find_map(|(id, placement)| {
			let (top, left, bottom) = match placement {
				Placement::Tiled { row, col } => (*row, *col, row + self.grids[&id].height as u32),
				Placement::Float { row, col, mouse: true, .. } => (*row, *col, row + self.grids[&id].height as u32),
				// Only its rows from `row` down show.
				Placement::Message { row, .. } => (*row, 0, screen_height),
				Placement::Float { .. } | Placement::Hidden => return None,
			};
			covers(id, top, left, bottom).then_some((id, top, left))
		});
		let (grid, top, left) = over.or_else(|| covers(1, 0, 0, screen_height).then_some((1, 0, 0)))?;
		Some(GridPosition { grid, row: row - top, col: col - left })
	}

	/// The cell of the screen where grid `id`'s top-left cell is shown, if the grid is shown
	pub fn grid_origin(&self, id: u32) -> Option<(u32, u32)> {
		if id == 1 {
			return self.grids.contains_key(&1).then_some((0, 0));
		}
		match self.placements.get(&id)? {
			Placement::Tiled { row, col } | Placement::Float { row, col, .. } => Some((*row, *col)),
			Placement::Message { row, .. } => Some((*row, 0)),
			Placement::Hidden => None,
		}
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
			// A grid starts with its first resize.
			RedrawNotification::GridResize(resizes) => {
				for resize in resizes {
					let grid = self.grids.entry(resize.grid).or_insert_with(|| Grid::new(0, 0));
					grid.resize(resize.width as usize, resize.height as usize);
					self.stale = true;
				}
			},
			RedrawNotification::GridDestroy(destroys) => {
				for destroy in destroys {
					self.grids.remove(&destroy.grid);
					self.viewports.remove(&destroy.grid);
					self.placements.remove(&destroy.grid);
					self.stale = true;
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
			RedrawNotification::WinPos(positions) => {
				for position in positions {
					self.place(position.grid, Placement::Tiled { row: position.start_row, col: position.start_col });
				}
			},
			RedrawNotification::WinFloatPos(positions) => {
				for position in positions {
					let (row, col, compindex) = (position.screen_row, position.screen_col, position.compindex);
					let mouse = position.mouse_enabled;
					self.place(position.grid, Placement::Float { row, col, compindex, mouse });
				}
			},
			RedrawNotification::WinHide(hides) => {
				for hide in hides {
					self.place(hide.grid, Placement::Hidden);
				}
			},
			RedrawNotification::WinClose(closes) => {
				for close in closes {
					self.place(close.grid, Placement::Hidden);
				}
			},
			RedrawNotification::MsgSetPos(positions) => {
				for position in positions {
					let separator = CellText::from(&position.sep_char)
						.ok()
						.filter(|_| position.scrolled && position.row > 0);
					self.place(
						position.grid,
						Placement::Message { row: position.row, compindex: position.compindex, separator },
					);
				}
			},
			RedrawNotification::WinViewport(viewports) => {
				for event in viewports {
					let viewport = self.viewports.entry(event.grid).or_default();
					let same_buffer = viewport.line_count == event.line_count;
					viewport.scrolled = if same_buffer { viewport.scrolled + event.scroll_delta } else { 0 };
					viewport.line_count = event.line_count;
				}
			},
			RedrawNotification::WinViewportMargins(margins) => {
				for event in margins {
					self.viewports.entry(event.grid).or_default().margins =
						[event.top, event.bottom, event.left, event.right];
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
			RedrawNotification::OptionSet(options) => {
				// Other fonts or line space change what's drawn, so a change is drawn with the next flush.
				for option in options {
					match (option.name.as_str(), option.value) {
						("guifont", OptionValue::String(value)) => {
							// Neovim sends its default with the options at attach, before the user's config can set
							// one.
							let default = self.default_guifont.get_or_insert_with(|| value.clone());
							let value = if value == *default { String::new() } else { value };
							if value != self.guifont {
								self.guifont = value;
								self.stale = true;
							}
						},
						("linespace", OptionValue::Number(value)) if value != self.linespace => {
							self.linespace = value;
							self.stale = true;
						},
						("mousemoveevent", OptionValue::Bool(value)) => self.mouse_move_events = value,
						_ => (),
					}
				}
			},
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
			RedrawNotification::MouseOn => self.mouse = true,
			RedrawNotification::MouseOff => self.mouse = false,
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

	fn place(&mut self, grid: u32, placement: Placement) {
		self.placements.insert(grid, placement);
		self.stale = true;
	}

	/// The layers to draw, if anything changed since the last `flush`
	fn flush(&mut self) -> Option<Redraw> {
		let redraw =
			std::mem::take(&mut self.stale).then(|| Redraw { background: self.background(), layers: self.layers() });
		// A scroll belongs to the redraw it's part of.
		for viewport in self.viewports.values_mut() {
			viewport.scrolled = 0;
		}
		redraw
	}

	/// The grids shown over grid 1, in Neovim's drawing order, like its own compositor (`:help
	/// ui-event-win_float_pos`): the windows, then floating windows and the message grid by `compindex`. Grids that
	/// aren't placed aren't shown.
	fn stacked(&self) -> Vec<(u32, &Placement)> {
		// Sort keys: tiled windows before floats and messages, then by Neovim's drawing order or grid id
		let mut placed: Vec<(u32, &Placement)> =
			self.placements.iter().map(|(&id, placement)| (id, placement)).collect();
		placed.retain(|(id, placement)| *placement != &Placement::Hidden && self.grids.contains_key(id));
		placed.sort_by_key(|&(id, placement)| match placement {
			Placement::Tiled { .. } | Placement::Hidden => (0, id),
			Placement::Float { compindex, .. } | Placement::Message { compindex, .. } => (1 + compindex, id),
		});
		placed
	}

	/// The layers in drawing order: grid 1, then the grids shown over it (see [`Editor::stacked`])
	fn layers(&self) -> Vec<Layer> {
		let screen_height = self.grids.get(&1).map_or(0, |grid| grid.height as u32);
		let placed = self.stacked();

		let mut layers = Vec::with_capacity(placed.len() + 2);
		if self.grids.contains_key(&1) {
			layers.push(self.layer(1, 0, 0));
		}
		for (id, placement) in placed {
			match placement {
				Placement::Tiled { row, col } | Placement::Float { row, col, .. } => {
					layers.push(self.layer(id, *row, *col))
				},
				Placement::Message { row, separator, .. } => {
					if let Some(separator) = separator {
						layers.push(self.separator(*row - 1, *separator));
					}
					let mut layer = self.layer(id, *row, 0);
					// The message grid is as tall as the screen; only its rows from `row` down show.
					clip_rows(&mut layer, screen_height.saturating_sub(*row));
					layers.push(layer);
				},
				Placement::Hidden => (),
			}
		}
		layers
	}

	/// Grid `id` as a layer at `row`, `col`, with the cursor if it's in this grid
	fn layer(&self, id: u32, row: u32, col: u32) -> Layer {
		let cursor = (id == self.cursor_grid).then(|| self.cursor.style()).flatten();
		let mut layer = cell_layer(id, &self.grids[&id], &self.highlights, cursor);
		(layer.row, layer.col) = (row, col);
		layer.scroll = self.scroll(id, layer.width, layer.height);
		Layer::Cells(layer)
	}

	/// How the text of grid `id`, `width` × `height` cells, scrolled since the last `flush`, if it did
	fn scroll(&self, id: u32, width: u32, height: u32) -> Option<Scroll> {
		let viewport = self.viewports.get(&id).filter(|viewport| viewport.scrolled != 0)?;
		let [top, bottom, left, right] = viewport.margins;
		Some(Scroll {
			rows: viewport.scrolled.clamp(i32::MIN.into(), i32::MAX.into()) as i32,
			top,
			bottom: height.saturating_sub(bottom),
			left,
			right: width.saturating_sub(right),
		})
	}

	/// The line the TUI draws above messages that scrolled over the windows: `separator` across the screen at `row`,
	/// in the `MsgSeparator` highlight
	fn separator(&self, row: u32, separator: CellText) -> Layer {
		let width = self.grids.get(&1).map_or(0, |grid| grid.width);
		let mut line = Grid::new(width, 1);
		let hl_id = Some(self.highlights.group("MsgSeparator"));
		line.set_line(0, 0, &[NvimGridCell { text: separator, hl_id, repeat: Some(width as u32) }]);

		let mut layer = cell_layer(0, &line, &self.highlights, None);
		// An id no grid has: Neovim's grid ids are 32-bit.
		(layer.id, layer.row) = (u64::from(u32::MAX) + 1, row);
		Layer::Cells(layer)
	}
}

/// Keeps the first `height` rows of `layer`, if it's taller
fn clip_rows(layer: &mut Layer, height: u32) {
	let Layer::Cells(CellLayer { height: layer_height, width, cells, cursor, .. }) = layer;
	if *layer_height <= height {
		return;
	}
	*layer_height = height;
	cells.truncate((height * *width) as usize);
	if cursor.is_some_and(|cursor| cursor.row >= height) {
		*cursor = None;
	}
}

#[cfg(test)]
mod tests {
	use nvim::{
		CellText, DefaultColorsSetEvent, GridCell, GridCursorGotoEvent, GridDestroyEvent, GridLineEvent,
		GridResizeEvent, HlAttrDefineEvent, HlAttrs, HlGroupSetEvent, MsgSetPosEvent, OptionSetEvent, OptionValue,
		RedrawNotification, WinCloseEvent, WinFloatPosEvent, WinHideEvent, WinPosEvent, WinViewportEvent,
		WinViewportMarginsEvent,
	};
	use renderer::{CellLayer, Cursor, CursorShape, Layer};
	use serde::de::IgnoredAny;

	use super::{Editor, GridPosition, Redraw};

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
		// A block cursor in the cell's swapped colors, on "b"
		let cursor = Cursor { row: 0, col: 1, width: 1, shape: CursorShape::Block, color: FG, text_color: BG };
		assert_eq!(layer.cursor, Some(cursor));

		// Moving the cursor is a change to draw.
		assert!(editor.apply(vec![goto(1, 0, 0), RedrawNotification::Flush]).is_some());
	}

	fn win_pos(grid: u32, row: u32, col: u32) -> RedrawNotification {
		let position = WinPosEvent { grid, win: IgnoredAny, start_row: row, start_col: col, width: 0, height: 0 };
		RedrawNotification::WinPos(vec![position])
	}

	fn float(grid: u32, row: u32, col: u32, compindex: u32) -> RedrawNotification {
		RedrawNotification::WinFloatPos(vec![WinFloatPosEvent {
			grid,
			win: IgnoredAny,
			anchor: String::from("NW"),
			anchor_grid: 1,
			anchor_row: f64::from(row),
			anchor_col: f64::from(col),
			mouse_enabled: true,
			zindex: 50,
			compindex,
			screen_row: row,
			screen_col: col,
		}])
	}

	fn messages(row: u32, scrolled: bool, compindex: u32) -> RedrawNotification {
		let sep_char = String::from("─");
		RedrawNotification::MsgSetPos(vec![MsgSetPosEvent { grid: 3, row, scrolled, sep_char, zindex: 200, compindex }])
	}

	/// Where each layer of `redraw` is: (id, row, col, height)
	fn placement(redraw: &Redraw) -> Vec<(u64, u32, u32, u32)> {
		redraw
			.layers
			.iter()
			.map(|Layer::Cells(layer)| (layer.id, layer.row, layer.col, layer.height))
			.collect()
	}

	fn flush(editor: &mut Editor, mut events: Vec<RedrawNotification>) -> Redraw {
		events.push(RedrawNotification::Flush);
		editor.apply(events).expect("something changed")
	}

	/// An editor with a 10×5 screen (grid 1) and a 4×3 window grid 2 that isn't placed yet
	fn screen() -> Editor {
		let mut editor = Editor::new();
		editor.apply(vec![colors(), resize(1, 10, 5), resize(2, 4, 3)]);
		editor
	}

	#[test]
	fn windows_are_drawn_once_placed() {
		let mut editor = screen();
		assert_eq!(placement(&flush(&mut editor, vec![])), [(1, 0, 0, 5)]);
		assert_eq!(placement(&flush(&mut editor, vec![win_pos(2, 0, 6)])), [(1, 0, 0, 5), (2, 0, 6, 3)]);
	}

	#[test]
	fn floats_come_after_windows_in_neovims_order() {
		let mut editor = screen();
		let events = vec![
			resize(6, 3, 2),
			resize(7, 3, 2),
			resize(9, 2, 2),
			float(6, 1, 1, 2),
			float(7, 2, 2, 1),
			win_pos(9, 3, 0),
			win_pos(2, 0, 6),
		];
		let ids: Vec<u64> = placement(&flush(&mut editor, events)).iter().map(|(id, ..)| *id).collect();
		assert_eq!(ids, [1, 2, 9, 7, 6]);
	}

	#[test]
	fn message_grid_shows_from_its_row_down() {
		let mut editor = screen();
		// Like Neovim, the message grid is as large as the screen.
		editor.apply(vec![resize(3, 10, 5), line(3, 0, "hello")]);
		assert_eq!(placement(&flush(&mut editor, vec![messages(4, false, 1)])), [(1, 0, 0, 5), (3, 4, 0, 1)]);

		// Scrolled up over the windows: a separator line in MsgSeparator above the messages
		let separator = HlAttrs { foreground: Some(0x00FF00), ..crate::highlight::tests::attrs() };
		let events = vec![
			RedrawNotification::HlAttrDefine(vec![HlAttrDefineEvent {
				id: 9,
				rgb_attr: separator.clone(),
				cterm_attr: separator,
				info: vec![],
			}]),
			RedrawNotification::HlGroupSet(vec![HlGroupSetEvent { name: String::from("MsgSeparator"), hl_id: 9 }]),
			messages(2, true, 1),
		];
		let redraw = flush(&mut editor, events);
		let separator_id = u64::from(u32::MAX) + 1;
		assert_eq!(placement(&redraw), [(1, 0, 0, 5), (separator_id, 1, 0, 1), (3, 2, 0, 3)]);

		let Layer::Cells(CellLayer { cells, .. }) = &redraw.layers[1];
		assert!(
			cells
				.iter()
				.all(|cell| cell.text.as_str() == "─" && cell.style.foreground == 0x00FF00)
		);
		assert_eq!(cells.len(), 10);
	}

	#[test]
	fn hidden_closed_and_destroyed_grids() {
		let mut editor = screen();
		flush(&mut editor, vec![win_pos(2, 0, 6)]);

		let hide = RedrawNotification::WinHide(vec![WinHideEvent { grid: 2 }]);
		assert_eq!(placement(&flush(&mut editor, vec![hide])), [(1, 0, 0, 5)]);
		assert_eq!(placement(&flush(&mut editor, vec![win_pos(2, 1, 6)])), [(1, 0, 0, 5), (2, 1, 6, 3)]);

		let close = RedrawNotification::WinClose(vec![WinCloseEvent { grid: 2 }]);
		let destroy = RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: 2 }]);
		assert_eq!(placement(&flush(&mut editor, vec![close, destroy])), [(1, 0, 0, 5)]);
		// A destroyed grid takes events no more.
		assert_eq!(editor.apply(vec![line(2, 0, "x"), RedrawNotification::Flush]), None);
	}

	#[test]
	fn cursor_is_drawn_in_its_window() {
		let mut editor = screen();
		let redraw = flush(&mut editor, vec![win_pos(2, 0, 6), line(2, 0, "ab"), goto(2, 0, 1)]);
		let cursor = |index: usize| {
			let Layer::Cells(layer) = &redraw.layers[index];
			layer.cursor.map(|cursor| (cursor.row, cursor.col))
		};
		// On "b" in the window, and not in grid 1
		assert_eq!((cursor(0), cursor(1)), (None, Some((0, 1))));
	}

	fn viewport(grid: u32, line_count: u32, scroll_delta: i64) -> RedrawNotification {
		RedrawNotification::WinViewport(vec![WinViewportEvent {
			grid,
			win: IgnoredAny,
			topline: 0,
			botline: 0,
			curline: 0,
			curcol: 0,
			line_count,
			scroll_delta,
		}])
	}

	/// The scroll of grid `id`'s layer in `redraw`: (rows, top, bottom, left, right)
	fn scroll_of(redraw: &Redraw, id: u64) -> Option<(i32, u32, u32, u32, u32)> {
		let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == id)?;
		layer
			.scroll
			.map(|scroll| (scroll.rows, scroll.top, scroll.bottom, scroll.left, scroll.right))
	}

	#[test]
	fn a_scroll_is_part_of_its_redraw() {
		let mut editor = screen();
		flush(&mut editor, vec![win_pos(2, 0, 0), viewport(2, 100, 0)]);

		// Scrolls add up until the redraw ends.
		let redraw = flush(&mut editor, vec![viewport(2, 100, 2), viewport(2, 100, 1), line(2, 0, "a")]);
		assert_eq!(scroll_of(&redraw, 2), Some((3, 0, 3, 0, 4)));
		assert_eq!(scroll_of(&redraw, 1), None);

		// The next redraw didn't scroll.
		assert_eq!(scroll_of(&flush(&mut editor, vec![line(2, 0, "b")]), 2), None);

		// A redraw without changes drops its scroll, so the next one doesn't slide.
		editor.apply(vec![viewport(2, 100, 1), RedrawNotification::Flush]);
		assert_eq!(scroll_of(&flush(&mut editor, vec![line(2, 0, "c")]), 2), None);
	}

	#[test]
	fn margins_dont_scroll() {
		let mut editor = screen();
		let margins = WinViewportMarginsEvent { grid: 2, win: IgnoredAny, top: 1, bottom: 0, left: 1, right: 1 };
		flush(
			&mut editor,
			vec![win_pos(2, 0, 0), RedrawNotification::WinViewportMargins(vec![margins]), viewport(2, 9, 0)],
		);
		let redraw = flush(&mut editor, vec![viewport(2, 9, -1), line(2, 1, "a")]);
		assert_eq!(scroll_of(&redraw, 2), Some((-1, 1, 3, 1, 3)));
	}

	#[test]
	fn another_buffer_doesnt_scroll() {
		let mut editor = screen();
		// Neovim reports the first viewport of a window, and switching buffers, as scrolls.
		let redraw = flush(&mut editor, vec![win_pos(2, 0, 0), viewport(2, 500, 243)]);
		assert_eq!(scroll_of(&redraw, 2), None);
		let redraw = flush(&mut editor, vec![viewport(2, 300, -294), line(2, 0, "a")]);
		assert_eq!(scroll_of(&redraw, 2), None);
	}

	fn option(name: &str, value: OptionValue) -> RedrawNotification {
		RedrawNotification::OptionSet(vec![OptionSetEvent { name: name.to_owned(), value }])
	}

	#[test]
	fn guifont_changes_are_drawn() {
		let mut editor = screen();
		// Neovim's default comes first, at attach.
		let default = || option("guifont", OptionValue::String(String::from("Source Code Pro,monospace")));
		flush(&mut editor, vec![default()]);
		assert_eq!(editor.guifont(), "");

		let guifont = || option("guifont", OptionValue::String(String::from("Iosevka:h14")));
		assert!(editor.apply(vec![guifont(), RedrawNotification::Flush]).is_some());
		assert_eq!(editor.guifont(), "Iosevka:h14");

		// The same value again, or another option, changes nothing to draw.
		assert_eq!(editor.apply(vec![guifont(), RedrawNotification::Flush]), None);
		assert_eq!(editor.apply(vec![option("ttimeoutlen", OptionValue::Number(50)), RedrawNotification::Flush]), None);

		// Set back to the default, it counts as unset again.
		assert!(editor.apply(vec![default(), RedrawNotification::Flush]).is_some());
		assert_eq!(editor.guifont(), "");
	}

	#[test]
	fn linespace_changes_are_drawn() {
		let mut editor = screen();
		flush(&mut editor, vec![option("linespace", OptionValue::Number(0))]);
		assert_eq!(editor.linespace(), 0);

		let linespace = || option("linespace", OptionValue::Number(3));
		assert!(editor.apply(vec![linespace(), RedrawNotification::Flush]).is_some());
		assert_eq!(editor.linespace(), 3);
		assert_eq!(editor.apply(vec![linespace(), RedrawNotification::Flush]), None);
	}

	#[test]
	fn mouse_on_and_off() {
		let mut editor = screen();
		assert!(!editor.mouse());
		editor.apply(vec![RedrawNotification::MouseOn]);
		assert!(editor.mouse());
		editor.apply(vec![RedrawNotification::MouseOff]);
		assert!(!editor.mouse());

		assert!(!editor.mouse_move_events());
		editor.apply(vec![option("mousemoveevent", OptionValue::Bool(true))]);
		assert!(editor.mouse_move_events());
	}

	#[test]
	fn the_grid_under_a_cell() {
		// The 10×5 screen, window grid 2 (4×3) at row 0, col 6, and float 6 (3×2) over it at row 1, col 5
		let mut editor = screen();
		flush(&mut editor, vec![resize(6, 3, 2), win_pos(2, 0, 6), float(6, 1, 5, 1)]);
		let at = |editor: &Editor, row, col| editor.grid_at(row, col);
		let cell = |grid, row, col| Some(GridPosition { grid, row, col });

		assert_eq!(at(&editor, 0, 7), cell(2, 0, 1));
		assert_eq!(at(&editor, 0, 2), cell(1, 0, 2));
		// The float covers the window.
		assert_eq!(at(&editor, 2, 6), cell(6, 1, 1));
		assert_eq!(at(&editor, 2, 9), cell(2, 2, 3));
		// Outside the screen
		assert_eq!(at(&editor, 5, 0), None);
		assert_eq!(at(&editor, 0, 10), None);

		// A float made with `mouse = false` lets clicks through to the window below.
		let mut no_mouse = float(6, 1, 5, 1);
		let RedrawNotification::WinFloatPos(positions) = &mut no_mouse else { unreachable!() };
		positions[0].mouse_enabled = false;
		flush(&mut editor, vec![no_mouse]);
		assert_eq!(at(&editor, 2, 6), cell(2, 2, 0));

		assert_eq!(
			(editor.grid_origin(1), editor.grid_origin(2), editor.grid_origin(6)),
			(Some((0, 0)), Some((0, 6)), Some((1, 5)))
		);
		assert_eq!(editor.grid_origin(9), None);
	}

	#[test]
	fn the_message_grid_covers_its_rows_only() {
		let mut editor = screen();
		flush(&mut editor, vec![resize(3, 10, 5), messages(3, false, 2)]);
		assert_eq!(editor.grid_at(3, 4), Some(GridPosition { grid: 3, row: 0, col: 4 }));
		assert_eq!(editor.grid_at(2, 4), Some(GridPosition { grid: 1, row: 2, col: 4 }));
		assert_eq!(editor.grid_origin(3), Some((3, 0)));
	}
}
