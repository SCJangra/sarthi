//! Neovim's UI state, from its redraw events, and the layers the renderer draws from it. Knows nothing about windows.

pub mod compositor;
pub mod cursor;
pub mod external;
pub mod grid;
pub mod highlight;

use std::collections::HashMap;

use renderer::{
	CellLayer, CellSize, Decoration, FontId, Layer, LayerShadow, Motion, Outline, RenderCell, Scroll, Shadow,
};
use rpc::neovim::{CellText, GridCell as NvimGridCell, OptionValue, RedrawNotification, Window};

use crate::compositor::cell_layer;
use crate::cursor::CursorState;
use crate::external::at_cursor;
pub use crate::external::{Anchor, Place, Relative};
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

	/// How each window's text moves when it scrolls, if not as the renderer's other scrolled text does: with its own
	/// motion, or jumping to its new place for `None`
	scroll_motions: HashMap<Window, Option<Motion>>,

	/// Whether the window is see-through, so the default background isn't drawn in cells (see [`Editor::see_through_cells`])
	see_through: bool,

	/// How each floating and external window looks besides its cells, and how one does until it has a style of its own
	floats: HashMap<Window, FloatStyle>,
	default_float: FloatStyle,

	/// Physical pixels in a logical one, which the float styles are in
	scale: f32,

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

	/// Where external windows go, by window; centered unless placed (see [`Editor::set_external_place`])
	places: HashMap<Window, Place>,

	/// Rows of the command line at the bottom of the screen (`'cmdheight'`), which external windows stay above: none
	/// until Lua says
	command_rows: u32,

	/// The fonts of external windows that have their own, and the size of their cells, by window (see
	/// [`Editor::set_external_font`])
	external_fonts: HashMap<Window, (FontId, CellSize)>,

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

/// How a floating or external window looks besides its cells, in logical pixels
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FloatStyle {
	/// The shadow it casts, if any, from its outer edge: its outline's, or a float's cells. Floats at one level share
	/// theirs (see [`Editor::float_shadows`]).
	pub shadow: Option<Shadow>,

	/// The line just outside its cells, if it has one. External windows only, like `corners` and `padding`: Neovim
	/// places floats in its cells, without room for them.
	pub outline: Option<OutlineStyle>,

	/// The radius of its rounded corners, if they are: its padding's, which goes around its cells
	pub corners: Option<f32>,

	/// The space around its cells, in its background, if it has any; its outline goes around it.
	pub padding: Option<[f32; 4]>,

	/// Its background, as `0xRRGGBB`, its padding's whatever its cells are highlighted with: Neovim's default one if
	/// `None`
	pub background: Option<u32>,

	/// How see-through its background is, from 0 to 100, like `winblend`
	pub blend: u8,

	/// The floating window it's attached to, like a scrollbar to its menu: it shares that window's shadow.
	pub parent: Option<Window>,
}

/// A line around a floating or external window, in logical pixels
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineStyle {
	pub width: f32,

	/// The highlight group whose text color the line has
	pub highlight: String,

	/// That color, as `0xRRGGBB`, if known: the editor only knows Neovim's UI groups' own
	pub color: Option<u32>,
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
			scroll_motions: HashMap::new(),
			floats: HashMap::new(),
			default_float: FloatStyle::default(),
			scale: 1.0,
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
			places: HashMap::new(),
			command_rows: 0,
			external_fonts: HashMap::new(),
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

	/// How `win`'s text moves when it scrolls: slides with `motion`, or jumps to its new place for `None`. Applies from
	/// the next scroll, even one already applied but not yet flushed. Until set, it slides as the renderer's other text.
	pub fn set_scroll_motion(&mut self, win: Window, motion: Option<Motion>) {
		self.scroll_motions.insert(win, motion);
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

	/// Physical pixels in a logical one, which the float styles are drawn in
	pub fn set_scale(&mut self, scale: f32) {
		self.stale |= self.scale != scale;
		self.scale = scale;
	}

	/// How floating and external windows look until they have a style of their own (see [`Editor::set_float_style`])
	pub fn set_default_float_style(&mut self, style: FloatStyle) {
		self.stale |= self.default_float != style;
		self.default_float = style;
	}

	/// Draws external window `win`'s text with the fonts of set `font`, in cells of `cell`, or with the main fonts for
	/// `None`: from now on until it closes
	pub fn set_external_font(&mut self, win: Window, font: Option<(FontId, CellSize)>) {
		let changed = match font {
			Some(font) => self.external_fonts.insert(win, font) != Some(font),
			None => self.external_fonts.remove(&win).is_some(),
		};
		self.stale |= changed;
	}

	/// The windows with fonts of their own (see [`Editor::set_external_font`])
	pub fn external_fonts(&self) -> impl Iterator<Item = Window> + '_ {
		self.external_fonts.keys().copied()
	}

	/// The fonts grid `id` is drawn with, and the size of its cells: an external window's own, or the main ones
	fn font_of(&self, id: u32) -> (FontId, CellSize) {
		match self.placements.get(&id) {
			Some(Placement::External { win, .. }) => self.external_fonts.get(win).copied(),
			_ => None,
		}
		.unwrap_or((FontId::MAIN, self.cell))
	}

	/// Size of grid `id`'s cells, in pixels (see [`Editor::font_of`])
	fn cell_of(&self, id: u32) -> CellSize {
		self.font_of(id).1
	}

	/// Where external window `win` goes, from now on until it closes. One placed by the cursor goes where the cursor is
	/// now.
	pub fn set_external_place(&mut self, win: Window, place: Place) {
		let cursor = self.grids.get(&self.cursor_grid).and_then(|grid| {
			let (row, col) = self.grid_origin(self.cursor_grid)?;
			Some((row + grid.cursor_row as u32, col + grid.cursor_col as u32))
		});
		let place = at_cursor(place, cursor.unwrap_or((0, 0)));
		self.stale |= self.places.insert(win, place) != Some(place);
	}

	/// Sets how many rows the command line takes at the bottom of the screen (`'cmdheight'`)
	pub fn set_command_rows(&mut self, rows: u32) {
		self.stale |= self.command_rows != rows;
		self.command_rows = rows;
	}

	/// How floating or external window `win` looks besides its cells, until it closes
	pub fn set_float_style(&mut self, win: Window, style: FloatStyle) {
		self.stale |= self.floats.get(&win) != Some(&style);
		self.floats.insert(win, style);
	}

	/// How floating or external window `win` looks besides its cells: its own style, or the default one until it has one
	fn style_of(&self, win: Window) -> &FloatStyle {
		self.floats.get(&win).unwrap_or(&self.default_float)
	}

	/// The shadow floating or external window `win` casts, if any, in physical pixels
	fn shadow_of(&self, win: Window) -> Option<Shadow> {
		let shadow = self.style_of(win).shadow?;
		let scale = self.scale;
		Some(Shadow { blur: shadow.blur * scale, offset: shadow.offset.map(|offset| offset * scale), ..shadow })
	}

	/// The outline of external window `win`, if it has one, in physical pixels: in the highlight group's
	/// text color
	fn outline_of(&self, win: Window) -> Option<Outline> {
		let outline = self.style_of(win).outline.as_ref()?;
		let group = || self.highlights.resolve(self.highlights.group(&outline.highlight)).foreground;
		let color = outline.color.unwrap_or_else(group);
		Some(Outline { width: outline.width * self.scale, color })
	}

	/// The radius of external window `win`'s corners, in physical pixels: 0 for square ones
	fn corners_of(&self, win: Window) -> f32 {
		self.style_of(win).corners.map_or(0.0, |radius| radius * self.scale)
	}

	/// The width of external window `win`'s outline in whole physical pixels, 0 without one
	fn outline_width(&self, win: Window) -> u32 {
		self.outline_of(win).map_or(0, |outline| outline.width.round().max(0.0) as u32)
	}

	/// External window `win`'s padding, in whole physical pixels: top, right, bottom and left
	fn padding_of(&self, win: Window) -> [u32; 4] {
		let padding = self.style_of(win).padding.unwrap_or([0.0; 4]);
		padding.map(|size| (size * self.scale).round().max(0.0) as u32)
	}

	/// How wide external window `win`'s frame is on each side, in pixels: its padding and outline; top, right,
	/// bottom and left
	fn frame_of(&self, win: Window) -> [u32; 4] {
		let outline = self.outline_width(win);
		self.padding_of(win).map(|padding| padding + outline)
	}

	/// What casts external window `win`'s shadow, from its cells at `rect` (left, top, width, height in
	/// pixels): its outer edge, its outline's around its padding
	fn shadow_area(&self, win: Window, rect: [u32; 4]) -> [u32; 4] {
		outset(rect, self.frame_of(win))
	}

	/// The radius of the corners of what casts external window `win`'s shadow (see
	/// [`Editor::shadow_area`]): its outline goes around its rounded corners, so they're rounder by its width.
	fn shadow_radius(&self, win: Window) -> f32 {
		let radius = self.corners_of(win);
		if radius > 0.0 { radius + self.outline_width(win) as f32 } else { 0.0 }
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

	/// The grid shown at `x`, `y` in pixels from the window's top-left corner, and the cell of it there: the topmost
	/// grid there that takes the mouse, so not a float made with `mouse = false` (`:help ui-multigrid`)
	pub fn grid_at(&self, x: f64, y: f64) -> Option<GridPosition> {
		if let Some(grid) = self.external_at(x, y) {
			let (row, col) = self.grid_cell(grid, x, y)?;
			return Some(GridPosition { grid, row: row as u32, col: col as u32 });
		}
		let (row, col) = self.screen_cell(x, y)?;
		self.grid_at_cell(row, col)
	}

	/// The grid shown at cell `row`, `col` of the screen, and that cell in it, among the grids on the screen's cells (not
	/// the external windows)
	fn grid_at_cell(&self, row: u32, col: u32) -> Option<GridPosition> {
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

	/// The topmost external window at `x`, `y` in pixels, by its grid
	fn external_at(&self, x: f64, y: f64) -> Option<u32> {
		self.stacked().into_iter().rev().find_map(|(id, placement)| {
			let Placement::External { win, .. } = placement else { return None };
			let [left, top, width, height] = self.external_rect(id, *win).map(f64::from);
			let inside = (left..left + width).contains(&x) && (top..top + height).contains(&y);
			inside.then_some(id)
		})
	}

	/// The cell of grid `id` at `x`, `y` in pixels from the window's top-left corner, as (row, column): negative or past
	/// the grid's edges when outside it, e.g. while dragging. `None` if the grid isn't shown.
	pub fn grid_cell(&self, id: u32, x: f64, y: f64) -> Option<(i64, i64)> {
		let ([left, top], cell) = match self.placements.get(&id) {
			Some(Placement::External { win, .. }) => {
				let [left, top, ..] = self.external_rect(id, *win);
				([f64::from(left), f64::from(top)], self.cell_of(id))
			},
			_ => {
				let (row, col) = self.grid_origin(id)?;
				([f64::from(col * self.cell.width), f64::from(row * self.cell.height)], self.cell)
			},
		};
		let row = ((y - top) / f64::from(cell.height)).floor() as i64;
		let col = ((x - left) / f64::from(cell.width)).floor() as i64;
		Some((row, col))
	}

	/// The cell of the screen at `x`, `y` in pixels, as (row, column), if it's on the screen's side of the window's
	/// top-left corner
	fn screen_cell(&self, x: f64, y: f64) -> Option<(u32, u32)> {
		let row = (y / f64::from(self.cell.height)).floor();
		let col = (x / f64::from(self.cell.width)).floor();
		(row >= 0.0 && col >= 0.0).then_some((row as u32, col as u32))
	}

	/// How the mouse pointer looks at `x`, `y` in pixels: text in windows, a resize arrow over separators and status
	/// lines, which can be dragged, and an arrow elsewhere, or while Neovim's mouse is off
	pub fn pointer_at(&self, x: f64, y: f64) -> Pointer {
		if !self.mouse {
			return Pointer::Arrow;
		}
		match self.grid_at(x, y) {
			Some(GridPosition { grid: 1, row, col }) => self.border_at(row, col),
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

	/// The cell of the screen where grid `id`'s top-left cell is shown, if the grid is shown on the screen's cells (an
	/// external window isn't)
	fn grid_origin(&self, id: u32) -> Option<(u32, u32)> {
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
						self.scroll_motions.remove(&win);
					}
					match self.placements.remove(&destroy.grid) {
						Some(Placement::Float { win, .. }) => _ = self.floats.remove(&win),
						Some(Placement::External { win, .. }) => {
							self.places.remove(&win);
							self.external_fonts.remove(&win);
						},
						_ => (),
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
		shadows.extend(self.external_shadows(&placed));
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
				// Placed by Neovim in its cells: a shadow, but no frame (outline, corners, padding), which Neovim
				// wouldn't leave room for
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
				Placement::External { win, .. } => layers.push(self.external_layer(id, *win, shadows.remove(&id))),
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
	/// `zindex` with the same shadow share it, so the windows of a plugin's UI read as one; a float attached to another
	/// counts at that float's level. A level's first shadow is drawn with its first window, under all of them; another,
	/// for windows with a shadow of their own, with the first window casting it.
	fn float_shadows(&self, placed: &[(u32, &Placement)]) -> HashMap<u32, LayerShadow> {
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

		// By level and shadow, in the order of their first windows: the level, the grid it's drawn with, and the shadow
		let mut groups: Vec<(u32, u32, LayerShadow)> = Vec::new();
		let mut first_of_level: HashMap<u32, u32> = HashMap::new();
		for &(id, win, _, [col, row]) in &floats {
			let level = level(win);
			let first = *first_of_level.entry(level).or_insert(id);
			let Some(shadow) = self.shadow_of(win) else { continue };
			let (grid, cell) = (&self.grids[&id], self.cell);
			let [width, height] = [grid.width as u32 * cell.width, grid.height as u32 * cell.height];
			// Its cells': a float has no frame
			let rect = [col * cell.width, row * cell.height, width, height];
			let same = groups
				.iter_mut()
				.find(|(group_level, _, group)| *group_level == level && group.shadow == shadow);
			match same {
				Some((_, _, group)) => group.area.push(rect),
				None => {
					let carrier = if groups.iter().any(|(group_level, ..)| *group_level == level) { id } else { first };
					// Square, like its cells
					groups.push((level, carrier, LayerShadow { shadow, area: vec![rect], radius: 0.0 }));
				},
			}
		}
		groups.into_iter().map(|(_, carrier, group)| (carrier, group)).collect()
	}

	/// Grid `id` as a layer at `row`, `col` of the screen, with the cursor if it's in this grid
	fn layer(&self, id: u32, row: u32, col: u32) -> Layer {
		let cursor = (id == self.cursor_grid).then(|| self.cursor.style()).flatten();
		let (font, cell) = self.font_of(id);
		let mut layer = cell_layer(id, &self.grids[&id], &self.highlights, cursor, cell);
		layer.font = font;
		(layer.x, layer.y) = (col * self.cell.width, row * self.cell.height);
		layer.scroll = self.scroll(id, layer.width, layer.height);
		Layer::Cells(layer)
	}

	/// The shadows under the external windows among `placed`, by the grid of the window each is drawn with. Like attached
	/// floats, external windows placed relative to another external window, and so on, share its shadow if it's the same,
	/// so windows put together read as one; it's drawn with the first of them, under all of them.
	fn external_shadows(&self, placed: &[(u32, &Placement)]) -> HashMap<u32, LayerShadow> {
		let grid_of = |win: Window| {
			let (&grid, _) = self.viewports.iter().find(|(_, viewport)| viewport.win == Some(win))?;
			matches!(self.placements.get(&grid), Some(Placement::External { .. })).then_some(grid)
		};
		// The external window it's placed relative to, if any, and on from there; a few at most, so a loop ends
		let root = |mut win: Window| {
			for _ in 0..8 {
				let Some(Place::Relative { relative: Relative::Win(next), .. }) = self.places.get(&win) else { break };
				if grid_of(*next).is_none() {
					break;
				}
				win = *next;
			}
			win
		};

		// By root and shadow, in the order of their first windows: the root, the grid it's drawn with, and the shadow
		let mut groups: Vec<(Window, u32, LayerShadow)> = Vec::new();
		for &(id, placement) in placed {
			let Placement::External { win, .. } = placement else { continue };
			let Some(shadow) = self.shadow_of(*win) else { continue };
			let rect = self.shadow_area(*win, self.external_rect(id, *win));
			let (root, radius) = (root(*win), self.shadow_radius(*win));
			match groups
				.iter_mut()
				.find(|(group_root, _, group)| *group_root == root && group.shadow == shadow)
			{
				Some((_, _, group)) => {
					group.area.push(rect);
					group.radius = group.radius.max(radius);
				},
				None => groups.push((root, id, LayerShadow { shadow, area: vec![rect], radius })),
			}
		}
		groups.into_iter().map(|(_, carrier, group)| (carrier, group)).collect()
	}

	/// External window `win`, grid `id`, as a layer where it's placed (see [`Editor::set_external_place`]), with `shadow`,
	/// the shadow drawn with it (see [`Editor::external_shadows`])
	fn external_layer(&self, id: u32, win: Window, shadow: Option<LayerShadow>) -> Layer {
		let mut layer = self.layer(id, 0, 0);
		let Layer::Cells(cells) = &mut layer;
		[cells.x, cells.y, ..] = self.external_rect(id, win);
		cells.shadow = shadow;
		cells.outline = self.outline_of(win);
		cells.corner_radius = self.corners_of(win);
		cells.padding = self.padding_of(win);
		let style = self.style_of(win);
		cells.padding_background = (style.background.unwrap_or_else(|| self.background()), style.blend);
		layer
	}

	/// Where external window `win`, grid `id`, is: left, top, width, height in pixels
	fn external_rect(&self, id: u32, win: Window) -> [u32; 4] {
		self.external_rect_within(id, win, 0)
	}

	/// [`Editor::external_rect`], `depth` external windows away from the one asked for: windows placed relative to one
	/// another can't go round in a loop.
	fn external_rect_within(&self, id: u32, win: Window, depth: u32) -> [u32; 4] {
		let (grid, cell) = (&self.grids[&id], self.cell_of(id));
		let size = [grid.width as u32 * cell.width, grid.height as u32 * cell.height];
		// Placed with its frame, like Neovim places a floating window with its border: its cells inside it
		let [top, right, bottom, left] = self.frame_of(win);
		let outer = [size[0] + left + right, size[1] + top + bottom];
		let [x, y] = self.external_place(win, outer, depth);
		[x + left, y + top, size[0], size[1]]
	}

	/// Where external window `win`, `outer` pixels with its frame, is placed: its frame's top-left corner. A window
	/// it's placed relative to counts from that window's text, inside its frame, border and winbar, like Neovim's
	/// floats do.
	fn external_place(&self, win: Window, outer: [u32; 2], depth: u32) -> [u32; 2] {
		// Above the command line, like Neovim keeps floating windows
		let screen = self.grids.get(&1).map_or([0, 0], |grid| {
			let rows = (grid.height as u32).saturating_sub(self.command_rows);
			[grid.width as u32 * self.cell.width, rows * self.cell.height]
		});
		let place = self.places.get(&win).copied().unwrap_or(Place::Center);
		// A window's text's top-left corner and cells, by its grid: every window has a viewport.
		let origin = |win: Window| {
			let (&grid, viewport) = self.viewports.iter().find(|(_, viewport)| viewport.win == Some(win))?;
			let [top, _, left, _] = viewport.margins;
			let (corner, cell) = match self.placements.get(&grid) {
				// In its own cells, where it's placed; past a few in a row, as if not shown
				Some(Placement::External { win, .. }) => {
					let [x, y, ..] = (depth < 8).then(|| self.external_rect_within(grid, *win, depth + 1))?;
					([x, y], self.cell_of(grid))
				},
				_ => {
					let (row, col) = self.grid_origin(grid)?;
					([col * self.cell.width, row * self.cell.height], self.cell)
				},
			};
			Some(([corner[0] + left * cell.width, corner[1] + top * cell.height], cell))
		};
		external::position(place, outer, screen, self.cell, f64::from(self.scale), origin)
	}

	/// How the text of grid `id`, `width` × `height` cells, scrolled since the last `flush`, if it did
	fn scroll(&self, id: u32, width: u32, height: u32) -> Option<Scroll> {
		let viewport = self.viewports.get(&id).filter(|viewport| viewport.scrolled != 0)?;
		let motion = viewport.win.and_then(|win| self.scroll_motions.get(&win).copied());
		// It jumps.
		if motion == Some(None) {
			return None;
		}
		let [top, bottom, left, right] = viewport.margins;
		Some(Scroll {
			rows: viewport.scrolled.clamp(i32::MIN.into(), i32::MAX.into()) as i32,
			top,
			bottom: height.saturating_sub(bottom),
			left,
			right: width.saturating_sub(right),
			motion: motion.flatten(),
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

/// `rect` (left, top, width, height in pixels) with as many pixels around it as given above, right, below and before it
/// (left), up to the window's top and left edges
fn outset([left, top, width, height]: [u32; 4], [above, right, below, before]: [u32; 4]) -> [u32; 4] {
	let (x, y) = (left.saturating_sub(before), top.saturating_sub(above));
	[x, y, left + width + right - x, top + height + below - y]
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
	use renderer::{CellLayer, CellSize, Cursor, CursorShape, FontId, Layer, Motion, Shadow};
	use rpc::neovim::{
		CellText, DefaultColorsSetEvent, GridCell, GridCursorGotoEvent, GridDestroyEvent, GridLineEvent,
		GridResizeEvent, HlAttrDefineEvent, HlAttrs, HlGroupSetEvent, MsgSetPosEvent, OptionSetEvent, OptionValue,
		RedrawNotification, WinCloseEvent, WinExternalPosEvent, WinFloatPosEvent, WinHideEvent, WinPosEvent,
		WinViewportEvent, WinViewportMarginsEvent, Window,
	};
	use serde::de::IgnoredAny;

	use super::{Anchor, Editor, FloatStyle, GridPosition, OutlineStyle, Place, Pointer, Redraw, Relative};

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

	/// The middle of screen cell `row`, `col`, in pixels
	fn middle(row: u32, col: u32) -> (f64, f64) {
		((f64::from(col) + 0.5) * f64::from(CELL.width), (f64::from(row) + 0.5) * f64::from(CELL.height))
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
		let (x, y) = middle(0, 0);
		assert_eq!(editor.pointer_at(x, y), Pointer::Arrow, "the mouse is off");

		editor.apply(vec![RedrawNotification::MouseOn]);
		let at = |row, col| {
			let (x, y) = middle(row, col);
			editor.pointer_at(x, y)
		};
		assert_eq!((at(0, 0), at(2, 3)), (Pointer::Text, Pointer::Text));
		assert_eq!((at(0, 4), at(2, 4)), (Pointer::ResizeColumns, Pointer::ResizeColumns));
		assert_eq!((at(3, 0), at(3, 3)), (Pointer::ResizeRows, Pointer::ResizeRows));
		// Where they meet, and away from both
		assert_eq!((at(3, 4), at(4, 8), at(1, 7)), (Pointer::ResizeRows, Pointer::Arrow, Pointer::Arrow));
	}

	#[test]
	fn external_windows_are_centered_over_everything() {
		let mut editor = screen();
		editor.set_default_float_style(FloatStyle {
			shadow: Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 }),
			..FloatStyle::default()
		});
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
	fn external_windows_go_where_placed() {
		let mut editor = screen();
		let at = |redraw: &Redraw| {
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 4).unwrap();
			(layer.x, layer.y)
		};
		// A 2×1 external window a row below window 2's top-left corner
		flush(&mut editor, vec![win_pos(2, 1, 6), viewport(2, 10, 0), resize(4, 2, 1), external(4)]);
		let below = Place::Relative {
			relative: Relative::Win(win_of(2)),
			anchor: Anchor::NorthWest,
			row: 1.0,
			col: 0.0,
			offset: [0.0; 2],
			fixed: false,
		};
		editor.set_external_place(win_of(4), below);
		assert_eq!(at(&editor.refresh().expect("placed")), (6 * 8, 2 * 16));

		// It follows the window.
		assert_eq!(at(&flush(&mut editor, vec![win_pos(2, 2, 0)])), (0, 3 * 16));

		// Placed by the cursor, it goes where the cursor is, and stays there.
		flush(&mut editor, vec![goto(1, 3, 4)]);
		let cursor = Place::Relative {
			relative: Relative::Cursor,
			anchor: Anchor::NorthWest,
			row: 1.0,
			col: 0.0,
			offset: [0.0; 2],
			fixed: false,
		};
		editor.set_external_place(win_of(4), cursor);
		assert_eq!(at(&editor.refresh().expect("placed")), (4 * 8, 4 * 16));
		assert_eq!(at(&flush(&mut editor, vec![goto(1, 0, 0)])), (4 * 8, 4 * 16));

		// Relative to external window 5, at the top left with cells of its own (10×20): in those cells
		let events = vec![viewport(5, 10, 0), resize(5, 2, 1), external(5)];
		flush(&mut editor, events);
		editor.set_external_font(win_of(5), Some((FontId::MAIN, CellSize { width: 10, height: 20 })));
		let top_left = Place::Relative {
			relative: Relative::Editor,
			anchor: Anchor::NorthWest,
			row: 0.0,
			col: 0.0,
			offset: [0.0; 2],
			fixed: false,
		};
		editor.set_external_place(win_of(5), top_left);
		let below = Place::Relative {
			relative: Relative::Win(win_of(5)),
			anchor: Anchor::NorthWest,
			row: 1.0,
			col: 2.0,
			offset: [0.0; 2],
			fixed: false,
		};
		editor.set_external_place(win_of(4), below);
		assert_eq!(at(&editor.refresh().expect("placed")), (2 * 10, 20));
		// Counted from 5's text, inside its padding, like from inside a float's border
		editor.set_float_style(win_of(5), FloatStyle { padding: Some([4.0; 4]), ..FloatStyle::default() });
		assert_eq!(at(&editor.refresh().expect("padded")), (2 * 10 + 4, 20 + 4));

		// Placed relative to each other: no loop, centered past a few
		editor.set_external_place(
			win_of(5),
			Place::Relative {
				relative: Relative::Win(win_of(4)),
				anchor: Anchor::NorthWest,
				row: 0.0,
				col: 0.0,
				offset: [0.0; 2],
				fixed: false,
			},
		);
		let _ = editor.refresh();
	}

	#[test]
	fn the_mouse_over_an_external_window() {
		let mut editor = screen();
		// A 4×2 external window, centered at (24, 24) on the 80×80 pixel screen, over window grid 2
		flush(&mut editor, vec![win_pos(2, 0, 0), resize(4, 4, 2), external(4), RedrawNotification::MouseOn]);
		let cell = |grid, row, col| Some(GridPosition { grid, row, col });
		assert_eq!(editor.grid_at(24.0, 24.0), cell(4, 0, 0));
		assert_eq!(editor.grid_at(24.0 + 3.0 * 8.0 + 1.0, 24.0 + 16.0 + 1.0), cell(4, 1, 3));
		assert_eq!(editor.pointer_at(30.0, 30.0), Pointer::Text);
		// Next to it, what's below it
		assert_eq!(editor.grid_at(23.0, 24.0), cell(2, 1, 2));

		// Its cells reach past it, e.g. for a drag: two rows and three columns before its top-left corner
		assert_eq!(editor.grid_cell(4, 0.0, 0.0), Some((-2, -3)));
		assert_eq!(editor.grid_cell(2, 8.0, 16.0), Some((1, 1)));
		assert_eq!(editor.grid_cell(9, 0.0, 0.0), None);
	}

	#[test]
	fn an_external_window_in_fonts_of_its_own() {
		let mut editor = screen();
		flush(&mut editor, vec![resize(4, 4, 2), external(4)]);
		// Cells of 10×20 pixels: 40×40 for its 4×2 cells, centered on the 80×80 pixel screen
		let cell = CellSize { width: 10, height: 20 };
		editor.set_external_font(win_of(4), Some((FontId::MAIN, cell)));
		assert_eq!(editor.external_fonts().collect::<Vec<_>>(), [win_of(4)]);
		let redraw = editor.refresh().expect("its cells changed");
		let Layer::Cells(window) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 4).unwrap();
		assert_eq!((window.x, window.y, window.cell), (20, 20, cell));
		// The mouse goes by its cells too.
		assert_eq!(editor.grid_at(20.0 + 10.0 + 1.0, 20.0 + 1.0), Some(GridPosition { grid: 4, row: 0, col: 1 }));

		// Back to the main fonts
		editor.set_external_font(win_of(4), None);
		let redraw = editor.refresh().expect("its cells changed");
		let Layer::Cells(window) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 4).unwrap();
		assert_eq!(window.cell, CELL);
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
	fn outlines_around_external_windows() {
		let mut editor = screen();
		let outline_of = |redraw: &Redraw, id: u64| {
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == id).unwrap();
			layer.outline
		};
		// A float and an external window; none until a style has one
		let events = vec![resize(3, 2, 1), float(3, 0, 0, 1), resize(4, 2, 1), external(4)];
		let redraw = flush(&mut editor, events);
		assert_eq!((outline_of(&redraw, 3), outline_of(&redraw, 4)), (None, None));

		// In the highlight group's text color, here the default one, in physical pixels
		let outline = |width| Some(OutlineStyle { width, highlight: String::from("FloatBorder"), color: None });
		editor.set_default_float_style(FloatStyle { outline: outline(2.0), ..FloatStyle::default() });
		editor.set_scale(1.5);
		let redraw = editor.refresh().expect("outlines changed");
		let drawn = Some(renderer::Outline { width: 3.0, color: FG });
		// Never around floats, which Neovim places without room for it, tiled windows or the screen
		assert_eq!((outline_of(&redraw, 3), outline_of(&redraw, 4)), (None, drawn));
		assert_eq!(outline_of(&redraw, 1), None);

		// A window's own style: one of its own, or none; a float's own still none
		editor.set_float_style(win_of(3), FloatStyle { outline: outline(3.0), ..FloatStyle::default() });
		editor.set_float_style(win_of(4), FloatStyle { outline: outline(3.0), ..FloatStyle::default() });
		let redraw = editor.refresh().expect("their styles changed");
		let thick = Some(renderer::Outline { width: 4.5, color: FG });
		assert_eq!((outline_of(&redraw, 3), outline_of(&redraw, 4)), (None, thick));
		editor.set_float_style(win_of(4), FloatStyle::default());
		assert_eq!(outline_of(&editor.refresh().expect("its style changed"), 4), None);

		// In the color Lua found for its group, which the editor may not know
		let green = OutlineStyle { width: 1.0, highlight: String::from("Comment"), color: Some(0x00FF00) };
		editor.set_float_style(win_of(4), FloatStyle { outline: Some(green), ..FloatStyle::default() });
		let redraw = editor.refresh().expect("its style changed");
		assert_eq!(outline_of(&redraw, 4), Some(renderer::Outline { width: 1.5, color: 0x00FF00 }));
	}

	#[test]
	fn rounded_corners_of_external_windows() {
		let mut editor = screen();
		let shadow = Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 });
		editor.set_default_float_style(FloatStyle { shadow, ..FloatStyle::default() });
		let radius_of = |redraw: &Redraw, id: u64| {
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == id).unwrap();
			(layer.corner_radius, layer.shadow.as_ref().map(|shadow| shadow.radius))
		};
		// A lone float, two floats side by side at another level, and an external window
		let events = vec![
			resize(3, 2, 1),
			float_at_level(3, 0, 0, 1, 50),
			resize(5, 2, 1),
			float_at_level(5, 2, 0, 2, 60),
			resize(6, 2, 1),
			float_at_level(6, 2, 2, 3, 60),
			resize(4, 2, 1),
			external(4),
		];
		let redraw = flush(&mut editor, events);
		assert_eq!(radius_of(&redraw, 3), (0.0, Some(0.0)), "square until a style rounds them");

		editor.set_default_float_style(FloatStyle { shadow, corners: Some(3.0), ..FloatStyle::default() });
		editor.set_scale(2.0);
		let redraw = editor.refresh().expect("corners changed");
		// In physical pixels, and the shadow too
		assert_eq!(radius_of(&redraw, 4), (6.0, Some(6.0)));
		// Never floats', which Neovim places in its cells, alone or side by side, nor the screen's or tiled windows'
		assert_eq!(radius_of(&redraw, 3), (0.0, Some(0.0)));
		assert_eq!((radius_of(&redraw, 5), radius_of(&redraw, 6)), ((0.0, Some(0.0)), (0.0, None)));
		assert_eq!(radius_of(&redraw, 1).0, 0.0);

		// A window's own radius, with the shadow round its outline
		let outline = Some(OutlineStyle { width: 1.0, highlight: String::from("FloatBorder"), color: None });
		editor.set_float_style(
			win_of(4),
			FloatStyle { shadow, outline, corners: Some(5.0), padding: None, parent: None, ..FloatStyle::default() },
		);
		let redraw = editor.refresh().expect("their styles changed");
		assert_eq!(radius_of(&redraw, 4), (10.0, Some(12.0)));
	}

	#[test]
	fn padding_around_external_windows() {
		let mut editor = screen();
		let shadow = Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 });
		editor.set_default_float_style(FloatStyle { shadow, ..FloatStyle::default() });
		let padding_of = |redraw: &Redraw, id: u64| {
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == id).unwrap();
			(layer.padding, layer.shadow.as_ref().map(|shadow| shadow.area.clone()))
		};
		// A float at row 1, column 1, and an external window at the top left; none until a style has it
		let events = vec![resize(3, 2, 1), float(3, 1, 1, 1), resize(4, 2, 1), external(4)];
		flush(&mut editor, events);
		let place = Place::Relative {
			relative: Relative::Editor,
			anchor: Anchor::NorthWest,
			row: 0.0,
			col: 0.0,
			offset: [0.0; 2],
			fixed: false,
		};
		editor.set_external_place(win_of(4), place);
		let redraw = editor.refresh().expect("placed");
		assert_eq!(padding_of(&redraw, 3), ([0; 4], Some(vec![[8, 16, 16, 16]])));
		assert_eq!(padding_of(&redraw, 4), ([0; 4], Some(vec![[0, 0, 16, 16]])));

		// In whole physical pixels; the shadow is cast from around it, and the outline around that.
		let outline = Some(OutlineStyle { width: 1.0, highlight: String::from("FloatBorder"), color: None });
		let style = FloatStyle { shadow, outline, padding: Some([3.0; 4]), ..FloatStyle::default() };
		editor.set_default_float_style(style.clone());
		editor.set_scale(2.0);
		let redraw = editor.refresh().expect("padding changed");
		// The external window's cells move 8 pixels away from its anchor (NW), so its frame starts where it's placed.
		assert_eq!(padding_of(&redraw, 4), ([6; 4], Some(vec![[0, 0, 32, 32]])));
		// Never a float's, which Neovim places without room for it, nor the screen's or tiled windows'
		assert_eq!(padding_of(&redraw, 3), ([0; 4], Some(vec![[8, 16, 16, 16]])));
		assert_eq!(padding_of(&redraw, 1).0, [0; 4]);

		// A window's own size, or none
		editor.set_float_style(win_of(4), FloatStyle { padding: Some([1.0; 4]), ..style.clone() });
		let redraw = editor.refresh().expect("its style changed");
		assert_eq!(padding_of(&redraw, 4).0, [2; 4]);

		// Each side its own: top 1, right 2, bottom 3, left 4, so 2, 4, 6 and 8 pixels, and the shadow grows by each
		// side's.
		editor.set_float_style(win_of(4), FloatStyle { padding: Some([1.0, 2.0, 3.0, 4.0]), ..style });
		let redraw = editor.refresh().expect("its padding changed");
		assert_eq!(padding_of(&redraw, 4), ([2, 4, 6, 8], Some(vec![[0, 0, 32, 28]])));
		editor.set_float_style(win_of(4), FloatStyle { shadow, ..FloatStyle::default() });
		assert_eq!(padding_of(&editor.refresh().expect("none"), 4).0, [0; 4]);

		// In the window's own background, whatever its cells', or Neovim's default one until known
		let background_of = |redraw: &Redraw| {
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 4).unwrap();
			layer.padding_background
		};
		editor.set_float_style(win_of(4), FloatStyle { padding: Some([1.0; 4]), ..FloatStyle::default() });
		assert_eq!(background_of(&editor.refresh().expect("padded")), (editor.background(), 0));
		let own =
			FloatStyle { padding: Some([1.0; 4]), background: Some(0x123456), blend: 30, ..FloatStyle::default() };
		editor.set_float_style(win_of(4), own);
		assert_eq!(background_of(&editor.refresh().expect("its background")), (0x123456, 30));
	}

	#[test]
	fn external_windows_are_laid_out_like_floats_with_a_border() {
		let mut editor = screen();
		// Window 2 at row 1 with a winbar, and a 2×1 external window with 4 pixels of padding, on the 80×80 pixel
		// screen
		let margins = WinViewportMarginsEvent { grid: 2, win: IgnoredAny, top: 1, bottom: 0, left: 0, right: 0 };
		let events = vec![
			resize(2, 10, 3),
			win_pos(2, 1, 0),
			viewport(2, 9, 0),
			RedrawNotification::WinViewportMargins(vec![margins]),
			resize(4, 2, 1),
			external(4),
		];
		flush(&mut editor, events);
		editor.set_float_style(win_of(4), FloatStyle { padding: Some([4.0; 4]), ..FloatStyle::default() });
		let at = |editor: &mut Editor, place| {
			editor.set_external_place(win_of(4), place);
			let redraw = editor.refresh().expect("placed");
			let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 4).unwrap();
			(layer.x, layer.y)
		};
		let place = |relative, row, col, fixed| Place::Relative {
			relative,
			anchor: Anchor::NorthWest,
			row,
			col,
			offset: [0.0; 2],
			fixed,
		};

		// From window 2's text, below its winbar; its frame there, its cells inside it
		assert_eq!(at(&mut editor, place(Relative::Win(win_of(2)), 0.0, 0.0, false)), (4, 2 * 16 + 4));
		// Kept on the screen with its frame: 24×24 pixels at most at 56, 56
		assert_eq!(at(&mut editor, place(Relative::Editor, 4.0, 9.0, false)), (60, 60));
		// Above the command line
		editor.set_command_rows(1);
		assert_eq!(at(&mut editor, place(Relative::Editor, 4.0, 9.0, false)), (60, 44));
		// `fixed`: not moved sideways, so cut off on the right
		assert_eq!(at(&mut editor, place(Relative::Editor, 4.0, 9.0, true)), (9 * 8 + 4, 44));
	}

	#[test]
	fn external_windows_placed_together_share_a_shadow() {
		let mut editor = screen();
		let shadow = Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 };
		editor.set_default_float_style(FloatStyle { shadow: Some(shadow), ..FloatStyle::default() });
		// 2×1 external windows: 4 at the top left, 5 just below it, and 6 on its own, centered
		let mut events = vec![];
		for id in 4..=6 {
			events.extend([viewport(id, 10, 0), resize(id, 2, 1), external(id)]);
		}
		flush(&mut editor, events);
		let at = |relative, row| Place::Relative {
			relative,
			anchor: Anchor::NorthWest,
			row,
			col: 0.0,
			offset: [0.0; 2],
			fixed: false,
		};
		editor.set_external_place(win_of(4), at(Relative::Editor, 0.0));
		editor.set_external_place(win_of(5), at(Relative::Win(win_of(4)), 1.0));
		let redraw = editor.refresh().expect("placed");

		// One shadow under 4 and 5, drawn with 4, the first; 6 has its own.
		assert_eq!(shadow_of(&redraw, 4), Some(vec![[0, 0, 2, 1], [0, 1, 2, 1]]));
		assert_eq!(shadow_of(&redraw, 5), None);
		assert_eq!(shadow_of(&redraw, 6).map(|area| area.len()), Some(1));
	}

	#[test]
	fn a_float_stays_in_its_cells() {
		let mut editor = screen();
		let outline = Some(OutlineStyle { width: 2.0, highlight: String::from("FloatBorder"), color: None });
		editor.set_default_float_style(FloatStyle { outline, padding: Some([2.0; 4]), ..FloatStyle::default() });
		// Placed at row 2, column 2: drawn there whatever its style, with no frame to move it
		let redraw = flush(&mut editor, vec![resize(3, 2, 1), float(3, 2, 2, 1)]);
		let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 3).unwrap();
		assert_eq!((layer.x, layer.y, layer.padding, layer.outline), (2 * 8, 2 * 16, [0; 4], None));
		// The mouse finds its cells where Neovim placed them.
		assert_eq!(editor.grid_at(17.0, 33.0), Some(GridPosition { grid: 3, row: 0, col: 0 }));
		assert_eq!(editor.grid_at(25.0, 33.0), Some(GridPosition { grid: 3, row: 0, col: 1 }));
	}

	#[test]
	fn the_shadow_is_cast_from_the_outline() {
		let mut editor = screen();
		let shadow = Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 };
		let outline = Some(OutlineStyle { width: 2.0, highlight: String::from("FloatBorder"), color: None });
		editor.set_default_float_style(FloatStyle {
			shadow: Some(shadow),
			outline,
			corners: None,
			padding: None,
			parent: None,
			..FloatStyle::default()
		});
		flush(&mut editor, vec![resize(3, 2, 1), external(3)]);
		let place = Place::Relative {
			relative: Relative::Editor,
			anchor: Anchor::NorthWest,
			row: 1.0,
			col: 1.0,
			offset: [0.0; 2],
			fixed: false,
		};
		editor.set_external_place(win_of(3), place);
		let redraw = editor.refresh().expect("placed");

		// The external window's cells and 2 pixels around them, which the outline covers: it doesn't hide the shadow's
		// edge. The cells moved by those 2 pixels from where it's placed, so the outline starts there.
		let Layer::Cells(layer) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 3).unwrap();
		let area = layer.shadow.as_ref().map(|shadow| shadow.area.clone());
		assert_eq!(area, Some(vec![[CELL.width, CELL.height, 2 * CELL.width + 4, CELL.height + 4]]));
	}

	#[test]
	fn floating_windows_have_the_shadow() {
		let shadow = Shadow { blur: 8.0, offset: [0.0, 2.0], color: 0, opacity: 0.5 };
		let mut editor = screen();
		flush(&mut editor, vec![resize(3, 2, 1), win_pos(2, 0, 0), float(3, 1, 1, 1)]);

		// None until set
		let redraw = flush(&mut editor, vec![line(3, 0, "a")]);
		assert_eq!(shadow_of(&redraw, 3), None);

		editor.set_default_float_style(FloatStyle {
			shadow: Some(shadow),
			outline: None,
			corners: None,
			padding: None,
			parent: None,
			..FloatStyle::default()
		});
		let redraw = flush(&mut editor, vec![line(3, 0, "b")]);
		// The float has it, under its own cells; the window doesn't.
		assert_eq!((shadow_of(&redraw, 3), shadow_of(&redraw, 2)), (Some(vec![[1, 1, 2, 1]]), None));

		// A float left out, until it isn't: either is a change to draw.
		editor.set_float_style(
			win_of(3),
			FloatStyle {
				shadow: None,
				outline: None,
				corners: None,
				padding: None,
				parent: None,
				..FloatStyle::default()
			},
		);
		assert_eq!(shadow_of(&flush(&mut editor, vec![]), 3), None);
		editor.set_float_style(
			win_of(3),
			FloatStyle {
				shadow: Some(shadow),
				outline: None,
				corners: None,
				padding: None,
				parent: None,
				..FloatStyle::default()
			},
		);
		assert!(shadow_of(&flush(&mut editor, vec![]), 3).is_some());

		// Changes on sarthi's side show without waiting for Neovim's next redraw, and only if there are any.
		assert_eq!(editor.refresh(), None);
		editor.set_float_style(
			win_of(3),
			FloatStyle {
				shadow: None,
				outline: None,
				corners: None,
				padding: None,
				parent: None,
				..FloatStyle::default()
			},
		);
		let redraw = editor.refresh().expect("the shadow changed");
		assert_eq!(shadow_of(&redraw, 3), None);

		// Forgotten with the float's grid
		flush(&mut editor, vec![RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: 3 }])]);
		assert!(editor.floats.is_empty());
	}

	#[test]
	fn floats_at_one_level_share_a_shadow() {
		let mut editor = screen();
		let shadow = Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 };
		editor.set_default_float_style(FloatStyle {
			shadow: Some(shadow),
			outline: None,
			corners: None,
			padding: None,
			parent: None,
			..FloatStyle::default()
		});
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
		editor.set_float_style(
			win_of(5),
			FloatStyle {
				shadow: Some(shadow),
				outline: None,
				corners: None,
				padding: None,
				parent: Some(win_of(3)),
				..FloatStyle::default()
			},
		);
		editor.set_float_style(
			win_of(7),
			FloatStyle {
				shadow: None,
				outline: None,
				corners: None,
				padding: None,
				parent: None,
				..FloatStyle::default()
			},
		);
		let redraw = flush(&mut editor, vec![]);

		// One shadow for 3, 4 and the attached 5, drawn with the first of them
		assert_eq!(shadow_of(&redraw, 3), Some(vec![[1, 1, 2, 1], [3, 1, 2, 1], [5, 1, 2, 1]]));
		assert_eq!((shadow_of(&redraw, 4), shadow_of(&redraw, 5), shadow_of(&redraw, 7)), (None, None, None));
		// The hover has its own.
		assert_eq!(shadow_of(&redraw, 6), Some(vec![[2, 0, 2, 1]]));

		// 4 with a shadow of its own: one for it, drawn with it, over 3's
		let own = Shadow { opacity: 0.8, ..shadow };
		editor.set_float_style(
			win_of(4),
			FloatStyle {
				shadow: Some(own),
				outline: None,
				corners: None,
				padding: None,
				parent: None,
				..FloatStyle::default()
			},
		);
		let redraw = flush(&mut editor, vec![]);
		assert_eq!(shadow_of(&redraw, 3), Some(vec![[1, 1, 2, 1], [5, 1, 2, 1]]));
		assert_eq!(shadow_of(&redraw, 4), Some(vec![[3, 1, 2, 1]]));
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
	fn some_windows_jump_or_slide_their_own_way() {
		let mut editor = screen();
		let motion = Motion::Spring { stiffness: 100.0, damping: 20.0 };
		flush(&mut editor, vec![win_pos(2, 0, 0), viewport(2, 100, 0)]);

		// Told before the redraw that scrolls, like `WinScrolled` does
		editor.set_scroll_motion(win_of(2), None);
		let redraw = flush(&mut editor, vec![viewport(2, 100, 1), line(2, 0, "a")]);
		assert_eq!(scroll_of(&redraw, 2), None);

		editor.set_scroll_motion(win_of(2), Some(motion));
		let redraw = flush(&mut editor, vec![viewport(2, 100, 1), line(2, 0, "b")]);
		assert_eq!(scroll_of(&redraw, 2), Some((1, 0, 3, 0, 4)));
		let Some(Layer::Cells(layer)) = redraw.layers.iter().find(|Layer::Cells(layer)| layer.id == 2) else {
			panic!()
		};
		assert_eq!(layer.scroll.and_then(|scroll| scroll.motion), Some(motion));

		// Forgotten with the window's grid
		editor.set_scroll_motion(win_of(2), None);
		flush(&mut editor, vec![RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: 2 }])]);
		assert!(editor.scroll_motions.is_empty());
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
		let at = |editor: &Editor, row, col| {
			let (x, y) = middle(row, col);
			editor.grid_at(x, y)
		};
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
		let at = |row, col| {
			let (x, y) = middle(row, col);
			editor.grid_at(x, y)
		};
		assert_eq!(at(3, 4), Some(GridPosition { grid: 3, row: 0, col: 4 }));
		assert_eq!(at(2, 4), Some(GridPosition { grid: 1, row: 2, col: 4 }));
		assert_eq!(editor.grid_origin(3), Some((3, 0)));
	}
}
