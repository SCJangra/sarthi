//! Neovim's UI state, from its redraw events, and the layers the renderer draws from it. Knows nothing about windows.

pub mod compositor;
pub mod cursor;
pub mod grid;
pub mod highlight;

use std::collections::{HashMap, HashSet};

use renderer::{CellLayer, CellSize, Decoration, Layer, LayerShadow, RenderCell, Scroll, Shadow};
use rpc::neovim::{CellText, GridCell as NvimGridCell, OptionValue, RedrawNotification, Window};

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

	/// Windows whose text jumps to its new place when it scrolls, rather than sliding
	jumping: HashSet<Window>,

	/// Whether the window is see-through, so the default background isn't drawn in cells (see [`Editor::see_through_cells`])
	see_through: bool,

	/// The shadow under floating windows, if any, and what decides each float's part in it
	shadow: Option<Shadow>,
	floats: HashMap<Window, FloatShadow>,

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

	/// Size of the screen's cells, in pixels, which places the layers in the window
	cell: CellSize,

	/// The `order` of the next external window shown (see [`Placement::External`])
	external_order: u64,

	/// Whether anything changed since the last `flush`, so it's drawn on the next one
	stale: bool,
}

/// Where a grid is shown, from Neovim's multigrid events
#[derive(Debug, Clone, PartialEq)]
enum Placement {
	/// A window at a cell of grid 1 (`win_pos`)
	Tiled { row: u32, col: u32 },

	/// Floating window `win` at a cell of the screen, in Neovim's drawing order of floats (`win_float_pos`). Without
	/// `mouse`, clicks go to what's below it.
	Float { win: Window, row: u32, col: u32, zindex: u32, compindex: u32, mouse: bool },

	/// External window `win` (`win_external_pos`), which sarthi places itself: centered over the screen, over the floating
	/// windows. Neovim ignores their `zindex`; they stack in the `order` they were first shown in.
	External { win: Window, order: u64 },

	/// The message grid, across the screen from `row` down (`msg_set_pos`). When messages scrolled over the windows,
	/// a line of `separator` goes above them.
	Message { row: u32, compindex: u32, separator: Option<CellText> },

	/// Not shown, e.g. a window of another tab (`win_hide`)
	Hidden,
}

/// How a window's text scrolls, from `win_viewport` and `win_viewport_margins`
#[derive(Debug, Default)]
struct Viewport {
	/// The window, from `win_viewport`
	win: Option<Window>,

	/// Lines in the window's buffer. When it changes, the window likely shows another buffer, which Neovim also
	/// reports as a scroll.
	line_count: u32,

	/// Rows and columns at the window's edges that don't scroll, like the winbar or a border: top, bottom, left, right
	margins: [u32; 4],

	/// Rows the text moved up since the last `flush`, or down if negative
	scrolled: i64,
}

/// A floating window's part in the shadow under floating windows
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatShadow {
	/// Whether it casts one: not if left out by the settings, or if it's fully transparent
	pub casts: bool,

	/// The floating window it's attached to, like a scrollbar to its menu: it shares that window's shadow.
	pub parent: Option<Window>,
}

/// How the mouse pointer looks, by what's under it (see [`Editor::pointer_at`])
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
	Arrow,

	/// Over text
	Text,

	/// Over a separator between windows side by side, which drags left and right
	ResizeColumns,

	/// Over a status line, which drags up and down
	ResizeRows,
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
			jumping: HashSet::new(),
			shadow: None,
			floats: HashMap::new(),
			see_through: false,
			highlights: HighlightState::new(),
			cursor: CursorState::default(),
			cursor_grid: 1,
			guifont: String::new(),
			default_guifont: None,
			linespace: 0,
			mouse: false,
			mouse_move_events: false,
			// Until set, a cell is a pixel.
			cell: CellSize { width: 1, height: 1 },
			external_order: 0,
			stale: false,
		}
	}

	/// Sets the size of the screen's cells, in pixels: what the text is drawn with
	pub fn set_cell_size(&mut self, cell: CellSize) {
		self.stale |= self.cell != cell;
		self.cell = cell;
	}

	/// Neovim's default background, as `0xRRGGBB`
	pub fn background(&self) -> u32 {
		self.highlights.defaults().background
	}

	/// The `guifont` option (`:help 'guifont'`): which fonts to draw text with. Empty unless set to something other than
	/// Neovim's default (a list of common fonts, while sarthi has its own default font).
	pub fn guifont(&self) -> &str {
		&self.guifont
	}

	/// Whether `win`'s text slides when it scrolls, or jumps to its new place. Applies from the next scroll, even one
	/// already applied but not yet flushed.
	pub fn set_smooth_scroll(&mut self, win: Window, smooth: bool) {
		if smooth {
			self.jumping.remove(&win);
		} else {
			self.jumping.insert(win);
		}
	}

	/// Whether the window is see-through: then cells in the default background draw none, outside floating windows.
	pub fn set_see_through(&mut self, see_through: bool) {
		self.stale |= self.see_through != see_through;
		self.see_through = see_through;
	}

	/// Whether the window has the focus: without it, the cursor is drawn as an outline.
	pub fn set_focused(&mut self, focused: bool) {
		self.cursor.set_focused(focused);
		self.stale = true;
	}

	/// Whether the cursor's moves animate in insert and replace mode, and into and out of the command line
	pub fn set_cursor_animation(&mut self, insert_mode: bool, cmdline: bool) {
		self.cursor.set_animation(insert_mode, cmdline);
	}

	/// The layers to draw, if something set on sarthi's side changed since the last redraw, like the shadow: to show it
	/// without waiting for Neovim's next redraw. Call it between Neovim's redraws, as sarthi's Lua and settings arrive.
	pub fn refresh(&mut self) -> Option<Redraw> {
		self.flush()
	}

	/// The shadow under floating windows, in physical pixels, or `None` for none
	pub fn set_shadow(&mut self, shadow: Option<Shadow>) {
		self.shadow = shadow;
		self.stale = true;
	}

	/// Floating window `win`'s part in the shadow; until set, it casts one and isn't attached
	pub fn set_float_shadow(&mut self, win: Window, shadow: FloatShadow) {
		self.stale |= self.floats.insert(win, shadow) != Some(shadow);
	}

	/// A `guifont` value the way [`Editor::guifont`] gives it: empty if it's Neovim's default
	pub fn strip_default_guifont<'a>(&self, value: &'a str) -> &'a str {
		if self.default_guifont.as_deref() == Some(value) { "" } else { value }
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
				// An external window isn't on the screen's cells.
				Placement::Float { .. } | Placement::External { .. } | Placement::Hidden => return None,
			};
			covers(id, top, left, bottom).then_some((id, top, left))
		});
		let (grid, top, left) = over.or_else(|| covers(1, 0, 0, screen_height).then_some((1, 0, 0)))?;
		Some(GridPosition { grid, row: row - top, col: col - left })
	}

	/// How the mouse pointer looks over cell `row`, `col` of the screen: text in windows, a resize arrow over separators
	/// and status lines, which can be dragged, and an arrow elsewhere, or while Neovim's mouse is off
	pub fn pointer_at(&self, row: u32, col: u32) -> Pointer {
		if !self.mouse {
			return Pointer::Arrow;
		}
		match self.grid_at(row, col) {
			Some(GridPosition { grid: 1, .. }) => self.border_at(row, col),
			Some(_) => Pointer::Text,
			None => Pointer::Arrow,
		}
	}

	/// What of grid 1 cell `row`, `col` is: a window's separator, right of it, or its status line, below it
	fn border_at(&self, row: u32, col: u32) -> Pointer {
		let windows = self.placements.iter().filter_map(|(id, placement)| match placement {
			Placement::Tiled { row, col } => Some((*row, *col, self.grids.get(id)?)),
			_ => None,
		});
		let mut pointer = Pointer::Arrow;
		for (top, left, grid) in windows {
			let (bottom, right) = (top + grid.height as u32, left + grid.width as u32);
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

	/// The cell of the screen where grid `id`'s top-left cell is shown, if the grid is shown
	pub fn grid_origin(&self, id: u32) -> Option<(u32, u32)> {
		if id == 1 {
			return self.grids.contains_key(&1).then_some((0, 0));
		}
		match self.placements.get(&id)? {
			Placement::Tiled { row, col } | Placement::Float { row, col, .. } => Some((*row, *col)),
			Placement::Message { row, .. } => Some((*row, 0)),
			Placement::External { .. } | Placement::Hidden => None,
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
					if let Some(win) = self.viewports.remove(&destroy.grid).and_then(|viewport| viewport.win) {
						self.jumping.remove(&win);
					}
					if let Some(Placement::Float { win, .. }) = self.placements.remove(&destroy.grid) {
						self.floats.remove(&win);
					}
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
					let (win, zindex, mouse) = (position.win, position.zindex, position.mouse_enabled);
					self.place(position.grid, Placement::Float { win, row, col, zindex, compindex, mouse });
				}
			},
			RedrawNotification::WinExternalPos(positions) => {
				for position in positions {
					// Sent again on resizes and tab switches: the window keeps its place in the stack.
					let order = match self.placements.get(&position.grid) {
						Some(Placement::External { order, .. }) => *order,
						_ => {
							self.external_order += 1;
							self.external_order
						},
					};
					self.place(position.grid, Placement::External { win: position.win, order });
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
					viewport.win = Some(event.win);
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
					self.cursor.set_mode(change.mode_idx as usize, &change.mode);
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
		if redraw.is_some() {
			self.cursor.drawn();
		}
		// A scroll belongs to the redraw it's part of.
		for viewport in self.viewports.values_mut() {
			viewport.scrolled = 0;
		}
		redraw
	}

	/// The grids shown over grid 1, in Neovim's drawing order, like its own compositor (`:help
	/// ui-event-win_float_pos`): the windows, then floating windows and the message grid by `compindex`; and over all of
	/// them, the external windows in their order. Grids that aren't placed aren't shown.
	fn stacked(&self) -> Vec<(u32, &Placement)> {
		// Sort keys: tiled windows, floats and messages, then external windows; within each, by drawing order or grid id
		let mut placed: Vec<(u32, &Placement)> =
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
	fn layers(&self) -> Vec<Layer> {
		let screen_height = self.grids.get(&1).map_or(0, |grid| grid.height as u32);
		let placed = self.stacked();

		let mut shadows = self.float_shadows(&placed);
		// What covers grid 1, and what covers the tiled windows: column, row, width, height in cells
		let screen_width = self.grids.get(&1).map_or(0, |grid| grid.width as u32);
		let area_of = |kind: fn(&Placement) -> Option<[u32; 2]>| -> Vec<[u32; 4]> {
			placed
				.iter()
				.filter_map(|(id, placement)| Some((self.grids.get(id)?, kind(placement)?)))
				.map(|(grid, [col, row])| [col, row, grid.width as u32, grid.height as u32])
				.collect()
		};
		let tiled = area_of(|placement| match placement {
			Placement::Tiled { row, col } => Some([*col, *row]),
			_ => None,
		});
		let messages: Vec<[u32; 4]> = placed
			.iter()
			.filter_map(|(_, placement)| match placement {
				Placement::Message { row, .. } => Some([0, *row, screen_width, screen_height.saturating_sub(*row)]),
				_ => None,
			})
			.collect();
		let under_windows: Vec<[u32; 4]> = tiled.iter().chain(&messages).copied().collect();

		let mut layers = Vec::with_capacity(placed.len() + 2);
		if self.grids.contains_key(&1) {
			layers.push(self.see_through_cells(self.layer(1, 0, 0), (0, 0), &under_windows));
		}
		for (id, placement) in placed {
			match placement {
				Placement::Tiled { row, col } => {
					layers.push(self.see_through_cells(self.layer(id, *row, *col), (*row, *col), &messages))
				},
				Placement::Float { row, col, .. } => {
					let mut layer = self.layer(id, *row, *col);
					let Layer::Cells(cells) = &mut layer;
					cells.shadow = shadows.remove(&id);
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
				Placement::External { .. } => layers.push(self.external_layer(id)),
				Placement::Hidden => (),
			}
		}
		layers
	}

	/// `layer`, a tiled window, grid 1 or the message grid, with cells in the default background drawing no background of
	/// their own while the window is see-through, so what's behind the window shows. Floating windows keep theirs, or
	/// what's under them would show.
	///
	/// Its cells under `covered` (column, row, width, height in cells of the screen), what other grids show over it, are
	/// left blank then: like in Neovim's own compositor, a screen cell shows only its top grid. Neovim leaves old
	/// separators and status lines in grid 1 under the windows, which opaque windows hide. The layer is at `origin`, a
	/// row and column of the screen.
	fn see_through_cells(&self, mut layer: Layer, origin: (u32, u32), covered: &[[u32; 4]]) -> Layer {
		if !self.see_through {
			return layer;
		}
		let (Layer::Cells(cells), background) = (&mut layer, self.background());
		for cell in cells.cells.iter_mut().filter(|cell| cell.background == background) {
			cell.blend = 100;
		}

		let blank = CellText::from(" ").expect("a space fits in a cell");
		let (width, height, (origin_row, origin_col)) = (cells.width, cells.height, origin);
		for &[col, row, cover_width, cover_height] in covered {
			// In the layer's cells
			let (left, top) = (col.saturating_sub(origin_col).min(width), row.saturating_sub(origin_row));
			let right = (col + cover_width).saturating_sub(origin_col).clamp(left, width);
			let bottom = (row + cover_height).saturating_sub(origin_row).min(height);
			for layer_row in top..bottom {
				let start = layer_row * width;
				for cell in &mut cells.cells[(start + left) as usize..(start + right) as usize] {
					*cell = RenderCell { text: blank, blend: 100, decoration: Decoration::default(), ..*cell };
				}
			}
		}
		layer
	}

	/// The shadows under the floating windows among `placed`, by the grid of the window each is drawn with. Floats at one
	/// `zindex` share a shadow, so the windows of a plugin's UI read as one; a float attached to another counts at that
	/// float's level. Each shadow is drawn with its group's first window, under all of them.
	fn float_shadows(&self, placed: &[(u32, &Placement)]) -> HashMap<u32, LayerShadow> {
		let Some(shadow) = self.shadow else { return HashMap::new() };
		let floats: Vec<(u32, Window, u32, [u32; 2])> = placed
			.iter()
			.filter_map(|&(id, placement)| match placement {
				Placement::Float { win, row, col, zindex, .. } => Some((id, *win, *zindex, [*col, *row])),
				_ => None,
			})
			.collect();
		let zindex: HashMap<Window, u32> = floats.iter().map(|&(_, win, zindex, _)| (win, zindex)).collect();
		let parent = |win: &Window| {
			self.floats
				.get(win)
				.and_then(|float| float.parent)
				.filter(|parent| zindex.contains_key(parent))
		};
		// Up to the float it's attached to, if any, and on from there; at most once per float, so a loop ends
		let level = |mut win: Window| {
			for _ in 0..floats.len() {
				let Some(next) = parent(&win) else { break };
				win = next;
			}
			zindex[&win]
		};

		let mut groups: HashMap<u32, (u32, LayerShadow)> = HashMap::new();
		for &(id, win, _, [col, row]) in &floats {
			let (_, group) = groups
				.entry(level(win))
				.or_insert_with(|| (id, LayerShadow { shadow, area: Vec::new() }));
			if self.floats.get(&win).is_none_or(|float| float.casts) {
				let (grid, cell) = (&self.grids[&id], self.cell);
				let [width, height] = [grid.width as u32 * cell.width, grid.height as u32 * cell.height];
				group.area.push([col * cell.width, row * cell.height, width, height]);
			}
		}
		groups.into_values().filter(|(_, group)| !group.area.is_empty()).collect()
	}

	/// Grid `id` as a layer at `row`, `col` of the screen, with the cursor if it's in this grid
	fn layer(&self, id: u32, row: u32, col: u32) -> Layer {
		let cursor = (id == self.cursor_grid).then(|| self.cursor.style()).flatten();
		let mut layer = cell_layer(id, &self.grids[&id], &self.highlights, cursor, self.cell);
		(layer.x, layer.y) = (col * self.cell.width, row * self.cell.height);
		layer.scroll = self.scroll(id, layer.width, layer.height);
		Layer::Cells(layer)
	}

	/// External window grid `id` as a layer centered over the screen, with a shadow of its own if floats have one
	fn external_layer(&self, id: u32) -> Layer {
		let mut layer = self.layer(id, 0, 0);
		let Layer::Cells(cells) = &mut layer;
		let [_, _, width, height] = cells.rect();
		let (screen_width, screen_height) = self
			.grids
			.get(&1)
			.map_or((0, 0), |grid| (grid.width as u32, grid.height as u32));
		let (screen_width, screen_height) = (screen_width * self.cell.width, screen_height * self.cell.height);
		(cells.x, cells.y) = (screen_width.saturating_sub(width) / 2, screen_height.saturating_sub(height) / 2);
		cells.shadow = self.shadow.map(|shadow| LayerShadow { shadow, area: vec![cells.rect()] });
		layer
	}

	/// How the text of grid `id`, `width` × `height` cells, scrolled since the last `flush`, if it did
	fn scroll(&self, id: u32, width: u32, height: u32) -> Option<Scroll> {
		let jumps = |viewport: &Viewport| viewport.win.is_some_and(|win| self.jumping.contains(&win));
		let viewport = self
			.viewports
			.get(&id)
			.filter(|viewport| viewport.scrolled != 0 && !jumps(viewport))?;
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

		let mut layer = cell_layer(0, &line, &self.highlights, None, self.cell);
		// An id no grid has: Neovim's grid ids are 32-bit.
		(layer.id, layer.y) = (u64::from(u32::MAX) + 1, row * self.cell.height);
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
	use renderer::{CellLayer, CellSize, Cursor, CursorShape, Layer, Shadow};
	use rpc::neovim::{
		CellText, DefaultColorsSetEvent, GridCell, GridCursorGotoEvent, GridDestroyEvent, GridLineEvent,
		GridResizeEvent, HlAttrDefineEvent, HlAttrs, HlGroupSetEvent, MsgSetPosEvent, OptionSetEvent, OptionValue,
		RedrawNotification, WinCloseEvent, WinExternalPosEvent, WinFloatPosEvent, WinHideEvent, WinPosEvent,
		WinViewportEvent, WinViewportMarginsEvent, Window,
	};
	use serde::de::IgnoredAny;

	use super::{Editor, FloatShadow, GridPosition, Pointer, Redraw};

	const FG: u32 = 0xC7C7C7;
	const BG: u32 = 0x1C1C1C;
	const CELL: CellSize = CellSize { width: 8, height: 16 };

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
		let cursor =
			Cursor { row: 0, col: 1, width: 1, shape: CursorShape::Block, color: FG, text_color: BG, animate: true };
		assert_eq!(layer.cursor, Some(cursor));

		// Moving the cursor is a change to draw.
		assert!(editor.apply(vec![goto(1, 0, 0), RedrawNotification::Flush]).is_some());
	}

	fn win_pos(grid: u32, row: u32, col: u32) -> RedrawNotification {
		let position = WinPosEvent { grid, win: IgnoredAny, start_row: row, start_col: col, width: 0, height: 0 };
		RedrawNotification::WinPos(vec![position])
	}

	fn external(grid: u32) -> RedrawNotification {
		RedrawNotification::WinExternalPos(vec![WinExternalPosEvent { grid, win: win_of(grid) }])
	}

	fn float(grid: u32, row: u32, col: u32, compindex: u32) -> RedrawNotification {
		float_at_level(grid, row, col, compindex, 50)
	}

	/// Like [`float`], at `zindex`
	fn float_at_level(grid: u32, row: u32, col: u32, compindex: u32, zindex: u32) -> RedrawNotification {
		RedrawNotification::WinFloatPos(vec![WinFloatPosEvent {
			grid,
			win: win_of(grid),
			anchor: String::from("NW"),
			anchor_grid: 1,
			anchor_row: f64::from(row),
			anchor_col: f64::from(col),
			mouse_enabled: true,
			zindex,
			compindex,
			screen_row: row,
			screen_col: col,
		}])
	}

	fn messages(row: u32, scrolled: bool, compindex: u32) -> RedrawNotification {
		let sep_char = String::from("─");
		RedrawNotification::MsgSetPos(vec![MsgSetPosEvent { grid: 3, row, scrolled, sep_char, zindex: 200, compindex }])
	}

	/// Where each layer of `redraw` is, in cells of the screen: (id, row, col, height)
	fn placement(redraw: &Redraw) -> Vec<(u64, u32, u32, u32)> {
		redraw
			.layers
			.iter()
			.map(|Layer::Cells(layer)| (layer.id, layer.y / CELL.height, layer.x / CELL.width, layer.height))
			.collect()
	}

	fn flush(editor: &mut Editor, mut events: Vec<RedrawNotification>) -> Redraw {
		events.push(RedrawNotification::Flush);
		editor.apply(events).expect("something changed")
	}

	/// An editor with a 10×5 screen (grid 1) of [`CELL`]s and a 4×3 window grid 2 that isn't placed yet
	fn screen() -> Editor {
		let mut editor = Editor::new();
		editor.set_cell_size(CELL);
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

	/// The window of grid `grid`, in these tests
	fn win_of(grid: u32) -> Window {
		Window(1000 + i64::from(grid))
	}

	fn viewport(grid: u32, line_count: u32, scroll_delta: i64) -> RedrawNotification {
		RedrawNotification::WinViewport(vec![WinViewportEvent {
			grid,
			win: win_of(grid),
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

	#[test]
	fn a_see_through_window_shows_through_the_default_background() {
		let blends = |redraw: &Redraw, id: u64| {
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == id).unwrap();
			layer.cells.iter().map(|cell| cell.blend).collect::<Vec<_>>()
		};
		let mut editor = screen();
		flush(&mut editor, vec![resize(3, 2, 1), win_pos(2, 0, 0), float(3, 1, 1, 1)]);
		editor.set_see_through(true);
		// In the window, a cell with its own background stays opaque.
		let attrs = crate::highlight::tests::attrs;
		let rgb_attr = HlAttrs { background: Some(0x123456), ..attrs() };
		let colored = HlAttrDefineEvent { id: 1, rgb_attr, cterm_attr: attrs(), info: vec![] };
		let cell = GridCell { text: CellText::from("a").unwrap(), hl_id: Some(1), repeat: None };
		let line = GridLineEvent { grid: 2, row: 0, col_start: 0, cells: vec![cell], wrap: false };
		let events = vec![RedrawNotification::HlAttrDefine(vec![colored]), RedrawNotification::GridLine(vec![line])];
		let redraw = flush(&mut editor, events);

		assert_eq!(&blends(&redraw, 2)[..2], [0, 100]);
		assert!(blends(&redraw, 1).iter().all(|blend| *blend == 100));
		// The float keeps its background.
		assert_eq!(blends(&redraw, 3), [0, 0]);

		editor.set_see_through(false);
		assert!(blends(&flush(&mut editor, vec![]), 1).iter().all(|blend| *blend == 0));
	}

	#[test]
	fn a_see_through_window_hides_what_grid_1_has_under_it() {
		// Neovim leaves an old separator in grid 1, where window 2 now is.
		let mut editor = screen();
		flush(&mut editor, vec![line(1, 0, "  |"), line(1, 4, "  |"), win_pos(2, 0, 0)]);
		let grid_1 = |redraw: &Redraw| {
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 1).unwrap();
			layer
				.cells
				.iter()
				.take(3)
				.map(|cell| (cell.text.as_str().to_owned(), cell.blend))
				.collect::<Vec<_>>()
		};

		// Opaque, the window hides it.
		let redraw = flush(&mut editor, vec![line(2, 0, "a")]);
		assert_eq!(grid_1(&redraw)[2], (String::from("|"), 0));

		// See-through, grid 1 is blank under the window: only the window shows there.
		editor.set_see_through(true);
		let redraw = flush(&mut editor, vec![]);
		assert_eq!(grid_1(&redraw)[2], (String::from(" "), 100));
		// Outside the window, grid 1 keeps what it shows.
		let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 1).unwrap();
		assert_eq!(layer.cells[4 * 10 + 2].text.as_str(), "|");
	}

	#[test]
	fn the_pointer_follows_whats_under_it() {
		// A 4×3 window at the top-left of the 10×5 screen: its separator is column 4, its status line row 3.
		let mut editor = screen();
		flush(&mut editor, vec![win_pos(2, 0, 0)]);
		assert_eq!(editor.pointer_at(0, 0), Pointer::Arrow, "the mouse is off");

		editor.apply(vec![RedrawNotification::MouseOn]);
		let at = |row, col| editor.pointer_at(row, col);
		assert_eq!((at(0, 0), at(2, 3)), (Pointer::Text, Pointer::Text));
		assert_eq!((at(0, 4), at(2, 4)), (Pointer::ResizeColumns, Pointer::ResizeColumns));
		assert_eq!((at(3, 0), at(3, 3)), (Pointer::ResizeRows, Pointer::ResizeRows));
		// Where they meet, and away from both
		assert_eq!((at(3, 4), at(4, 8), at(1, 7)), (Pointer::ResizeRows, Pointer::Arrow, Pointer::Arrow));
	}

	#[test]
	fn external_windows_are_centered_over_everything() {
		let mut editor = screen();
		editor.set_shadow(Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 }));
		// A float and a 4×2 external window: the screen is 80×80 pixels, the window 32×32.
		let events = vec![resize(3, 2, 1), float(3, 0, 0, 5), resize(4, 4, 2), external(4)];
		let redraw = flush(&mut editor, events);
		let ids: Vec<u64> = redraw.layers.iter().map(|Layer::Cells(layer)| layer.id).collect();
		assert_eq!(ids, [1, 3, 4]);
		let Layer::Cells(window) = &redraw.layers[2];
		assert_eq!((window.x, window.y), (24, 24));
		// Its own shadow, under it
		assert_eq!(shadow_of(&redraw, 4), Some(vec![[3, 1, 4, 2]]));

		// A larger window is centered again.
		let redraw = flush(&mut editor, vec![resize(4, 8, 4), external(4)]);
		let Layer::Cells(window) = &redraw.layers[2];
		assert_eq!((window.x, window.y), (8, 8));
	}

	#[test]
	fn external_windows_keep_their_order() {
		let mut editor = screen();
		let events = vec![resize(4, 2, 1), resize(5, 2, 1), external(4), external(5)];
		flush(&mut editor, events);
		// Neovim sends the first again, e.g. after a tab switch: it stays under the second.
		let redraw = flush(&mut editor, vec![external(4)]);
		let ids: Vec<u64> = redraw.layers.iter().map(|Layer::Cells(layer)| layer.id).collect();
		assert_eq!(ids, [1, 4, 5]);

		// Closed, it's gone.
		let closed = RedrawNotification::WinClose(vec![WinCloseEvent { grid: 4 }]);
		let redraw = flush(&mut editor, vec![closed]);
		assert_eq!(redraw.layers.len(), 2);
	}

	#[test]
	fn layers_are_placed_in_pixels() {
		let mut editor = screen();
		let redraw = flush(&mut editor, vec![win_pos(2, 1, 6)]);
		let Layer::Cells(window) = &redraw.layers[1];
		assert_eq!((window.x, window.y, window.cell), (6 * 8, 16, CELL));

		// A new cell size moves and resizes them.
		let cell = CellSize { width: 10, height: 20 };
		editor.set_cell_size(cell);
		let redraw = editor.refresh().expect("the cell size changed");
		let Layer::Cells(window) = &redraw.layers[1];
		assert_eq!((window.x, window.y, window.cell), (60, 20, cell));
		assert_eq!(editor.refresh(), None, "nothing changed since");
	}

	#[test]
	fn floating_windows_have_the_shadow() {
		let shadow = Shadow { blur: 8.0, offset: [0.0, 2.0], color: 0, opacity: 0.5 };
		let mut editor = screen();
		flush(&mut editor, vec![resize(3, 2, 1), win_pos(2, 0, 0), float(3, 1, 1, 1)]);

		// None until set
		let redraw = flush(&mut editor, vec![line(3, 0, "a")]);
		assert_eq!(shadow_of(&redraw, 3), None);

		editor.set_shadow(Some(shadow));
		let redraw = flush(&mut editor, vec![line(3, 0, "b")]);
		// The float has it, under its own cells; the window doesn't.
		assert_eq!((shadow_of(&redraw, 3), shadow_of(&redraw, 2)), (Some(vec![[1, 1, 2, 1]]), None));

		// A float left out, until it isn't: either is a change to draw.
		editor.set_float_shadow(win_of(3), FloatShadow { casts: false, parent: None });
		assert_eq!(shadow_of(&flush(&mut editor, vec![]), 3), None);
		editor.set_float_shadow(win_of(3), FloatShadow { casts: true, parent: None });
		assert!(shadow_of(&flush(&mut editor, vec![]), 3).is_some());

		// Changes on sarthi's side show without waiting for Neovim's next redraw, and only if there are any.
		assert_eq!(editor.refresh(), None);
		editor.set_float_shadow(win_of(3), FloatShadow { casts: false, parent: None });
		let redraw = editor.refresh().expect("the shadow changed");
		assert_eq!(shadow_of(&redraw, 3), None);

		// Forgotten with the float's grid
		flush(&mut editor, vec![RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: 3 }])]);
		assert!(editor.floats.is_empty());
	}

	#[test]
	fn floats_at_one_level_share_a_shadow() {
		let mut editor = screen();
		editor.set_shadow(Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 }));
		// 3 and 4 side by side at 50, like a picker's windows; 5 at 52 attached to 3, like a scrollbar; 6 at 100, a hover
		// over them; 7 at 50, fully transparent
		let mut events: Vec<_> = (3..=7).map(|id| resize(id, 2, 1)).collect();
		events.extend([
			float_at_level(3, 1, 1, 1, 50),
			float_at_level(4, 1, 3, 2, 50),
			float_at_level(5, 1, 5, 3, 52),
			float_at_level(6, 0, 2, 4, 100),
			float_at_level(7, 3, 0, 5, 50),
		]);
		flush(&mut editor, events);
		editor.set_float_shadow(win_of(5), FloatShadow { casts: true, parent: Some(win_of(3)) });
		editor.set_float_shadow(win_of(7), FloatShadow { casts: false, parent: None });
		let redraw = flush(&mut editor, vec![]);

		// One shadow for 3, 4 and the attached 5, drawn with the first of them
		assert_eq!(shadow_of(&redraw, 3), Some(vec![[1, 1, 2, 1], [3, 1, 2, 1], [5, 1, 2, 1]]));
		assert_eq!((shadow_of(&redraw, 4), shadow_of(&redraw, 5), shadow_of(&redraw, 7)), (None, None, None));
		// The hover has its own.
		assert_eq!(shadow_of(&redraw, 6), Some(vec![[2, 0, 2, 1]]));
	}

	/// The area of the shadow drawn with grid `id`'s layer in `redraw`, in cells of the screen
	fn shadow_of(redraw: &Redraw, id: u64) -> Option<Vec<[u32; 4]>> {
		let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == id).unwrap();
		let cells = |[x, y, width, height]: [u32; 4]| {
			[x / CELL.width, y / CELL.height, width / CELL.width, height / CELL.height]
		};
		layer
			.shadow
			.as_ref()
			.map(|shadow| shadow.area.iter().copied().map(cells).collect())
	}

	#[test]
	fn some_windows_jump() {
		let mut editor = screen();
		flush(&mut editor, vec![win_pos(2, 0, 0), viewport(2, 100, 0)]);

		// Told before the redraw that scrolls, like `WinScrolled` does
		editor.set_smooth_scroll(win_of(2), false);
		let redraw = flush(&mut editor, vec![viewport(2, 100, 1), line(2, 0, "a")]);
		assert_eq!(scroll_of(&redraw, 2), None);

		editor.set_smooth_scroll(win_of(2), true);
		let redraw = flush(&mut editor, vec![viewport(2, 100, 1), line(2, 0, "b")]);
		assert_eq!(scroll_of(&redraw, 2), Some((1, 0, 3, 0, 4)));

		// Forgotten with the window's grid
		editor.set_smooth_scroll(win_of(2), false);
		flush(&mut editor, vec![RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: 2 }])]);
		assert!(editor.jumping.is_empty());
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
		assert_eq!(editor.strip_default_guifont("Source Code Pro,monospace"), "");
		assert_eq!(editor.strip_default_guifont("Iosevka"), "Iosevka");

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
