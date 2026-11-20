//! Neovim's UI state, from its redraw events, and the layers the renderer draws from it. Knows nothing about windows.

#![warn(missing_docs)]

pub mod compositor;
pub mod cursor;
pub mod external;
pub mod grid;
pub mod highlight;
mod layers;
mod pointer;
pub mod preedit;
mod redraw;
mod shadows;

use std::collections::HashMap;

use renderer::{CellSize, Edges, FontId, Layer, Motion, Outline, Rect, Shadow};
use rpc::neovim::{CellText, Grid, Window};
use smart_default::SmartDefault;

use crate::cursor::CursorState;
use crate::external::at_cursor;
pub use crate::external::{Anchor, Place, Relative};
use crate::grid::GridState;
use crate::highlight::HighlightState;
pub use crate::pointer::{GridPosition, Pointer};
use crate::preedit::Preedit;

/// Neovim's UI state, kept up to date from its redraw events (`:help ui-events`)
#[derive(Debug, SmartDefault)]
pub struct Editor {
	/// Neovim's grids (`:help ui-multigrid`). The global grid, [`Grid::GLOBAL`], is the screen: it holds what isn't in
	/// a window, like window separators, status lines and the command line, and the windows' grids are placed on it.
	grids: HashMap<Grid, GridState>,

	/// Where each grid other than grid 1 is shown; grids without one aren't shown yet
	placements: HashMap<Grid, Placement>,

	/// How each window's text scrolls, by grid
	viewports: HashMap<Grid, Viewport>,

	/// How each window's text moves when it scrolls, if not as the renderer's other scrolled text does: with its own
	/// motion, or jumping to its new place for `None`
	scroll_motions: HashMap<Window, Option<Motion>>,

	/// Whether the window is see-through, so the default background isn't drawn in cells (see [`Editor::see_through_cells`])
	see_through: bool,

	/// How each floating and external window looks besides its cells, and how one does until it has a style of its own
	floats: HashMap<Window, FloatStyle>,
	default_float: FloatStyle,

	/// Physical pixels in a logical one, which the float styles are in
	#[default(1.0)]
	scale: f32,

	highlights: HighlightState,
	cursor: CursorState,

	/// The grid the cursor is in
	#[default(Grid::GLOBAL)]
	cursor_grid: Grid,

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
	/// Until set, a cell is a pixel.
	#[default(CellSize { width: 1, height: 1 })]
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

	/// The text an input method is composing, shown at the cursor
	preedit: Option<Preedit>,

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

	/// Rows and columns at the window's edges that don't scroll, like the winbar or a border
	margins: Edges<u32>,

	/// Rows the text moved up since the last `flush`, or down if negative
	scrolled: i64,
}

/// How a floating or external window looks besides its cells, in logical pixels
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FloatStyle {
	/// The shadow it casts, if any, from its outer edge: its outline's, or a float's cells. Floats at one level share
	/// theirs (see `Editor::float_shadows`).
	pub shadow: Option<Shadow>,

	/// The line just outside its cells, if it has one. External windows only, like `corners` and `padding`: Neovim
	/// places floats in its cells, without room for them.
	pub outline: Option<OutlineStyle>,

	/// The radius of its rounded corners, if they are: its padding's, which goes around its cells
	pub corners: Option<f32>,

	/// The space around its cells, in its background, if it has any; its outline goes around it.
	pub padding: Option<Edges<f32>>,

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
	/// Thickness
	pub width: f32,

	/// The highlight group whose text color the line has
	pub highlight: String,

	/// That color, as `0xRRGGBB`, if known: the editor only knows Neovim's UI groups' own
	pub color: Option<u32>,
}

/// What to draw after Neovim finished redrawing
#[derive(Debug, Clone, PartialEq)]
pub struct Redraw {
	/// Neovim's default background, as `0xRRGGBB`, for the parts of a window no grid covers
	pub background: u32,

	/// Drawn in order, so later ones cover earlier ones
	pub layers: Vec<Layer>,
}

impl Editor {
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

	/// Shows `preedit`, the text an input method is composing, at the cursor from the next redraw on; `None` once it's
	/// committed or abandoned
	pub fn set_preedit(&mut self, preedit: Option<Preedit>) {
		let preedit = preedit.filter(|preedit| !preedit.text.is_empty());
		self.stale |= self.preedit != preedit;
		self.preedit = preedit;
	}

	/// Whether an input method is composing text, whose keys are its own
	pub fn is_composing(&self) -> bool {
		self.preedit.is_some()
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
	fn font_of(&self, id: Grid) -> (FontId, CellSize) {
		match self.placements.get(&id) {
			Some(Placement::External { win, .. }) => self.external_fonts.get(win).copied(),
			_ => None,
		}
		.unwrap_or((FontId::MAIN, self.cell))
	}

	/// Size of grid `id`'s cells, in pixels (see [`Editor::font_of`])
	fn cell_of(&self, id: Grid) -> CellSize {
		self.font_of(id).1
	}

	/// Where external window `win` goes, from now on until it closes. One placed by the cursor goes where the cursor is
	/// now.
	pub fn set_external_place(&mut self, win: Window, place: Place) {
		let cursor = self.grids.get(&self.cursor_grid).and_then(|grid| {
			let (row, col) = self.grid_origin(self.cursor_grid)?;
			Some((row + grid.cursor_row, col + grid.cursor_col))
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

	/// External window `win`'s padding, in whole physical pixels
	fn padding_of(&self, win: Window) -> Edges<u32> {
		let padding = self.style_of(win).padding.unwrap_or_default();
		padding.map(|size| (size * self.scale).round().max(0.0) as u32)
	}

	/// How wide external window `win`'s frame is on each side, in pixels: its padding and outline
	fn frame_of(&self, win: Window) -> Edges<u32> {
		let outline = self.outline_width(win);
		self.padding_of(win).map(|padding| padding + outline)
	}

	/// What casts external window `win`'s shadow, from its cells at `rect` in pixels: its outer edge, its outline's
	/// around its padding
	fn shadow_area(&self, win: Window, rect: Rect<u32>) -> Rect<u32> {
		rect.outset(self.frame_of(win))
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

	/// A `guifont` value to set, from one the way [`Editor::guifont`] gives it: Neovim's default if empty
	pub fn with_default_guifont(&self, value: String) -> String {
		match &self.default_guifont {
			Some(default) if value.is_empty() => default.clone(),
			_ => value,
		}
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
}

#[cfg(test)]
mod tests;
