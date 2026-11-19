//! What the renderer draws: [`Layer`]s of cells, with their cursor, scroll, shadow and frame

use arrayvec::ArrayString;

use crate::{CellSize, Edges, FontId, Motion, Rect};

/// A block of cells at a position in the window, e.g. one of Neovim's grids, each cell `cell` large
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
	/// Identifies the layer across frames, so unchanged rows aren't laid out again
	pub id: u64,

	/// Position of the top-left cell, in pixels from the window's top-left corner
	pub x: u32,
	/// Top edge, in pixels from the window's top
	pub y: u32,

	/// The fonts its text is drawn with, which also give its cells' size
	pub font: FontId,

	/// Size of each cell, in pixels: a layer's text is laid out in cells of this size
	pub cell: CellSize,

	/// Size in cells
	pub width: u32,
	/// Rows of cells
	pub height: u32,

	/// Row by row, `width * height` cells
	pub cells: Vec<RenderCell>,

	/// The cursor, drawn over the text where the layer's cells are, even while scrolled text slides
	pub cursor: Option<Cursor>,

	/// How the text moved since the layer was last shown, so the renderer can slide it to its new place
	pub scroll: Option<Scroll>,

	/// A shadow under the layer, and under others drawn with it: e.g. under the floating windows of one plugin's UI, drawn
	/// with the first of them
	pub shadow: Option<LayerShadow>,

	/// A line around the layer, just outside its cells, so it takes none of them
	pub outline: Option<Outline>,

	/// Pixels of its background around its cells, taking none of them. The outline, the
	/// rounded corners and the shadow go around it.
	pub padding: Edges<u32>,

	/// The background around its cells, the window's own as `0xRRGGBB`, and how see-through it is, like a cell's `blend`:
	/// its padding's, whatever the cells next to it are highlighted with
	pub padding_background: (u32, u8),

	/// Radius of its rounded corners, in pixels; 0 for square ones. What's drawn in its cells is cut to them, except text.
	pub corner_radius: f32,
}

impl Layer {
	/// The layer's rectangle, in pixels from the window's top-left corner
	pub fn rect(&self) -> Rect<u32> {
		Rect::new(self.x, self.y, self.width * self.cell.width, self.height * self.cell.height)
	}

	/// Its rectangle with its padding around it, in pixels, the left and top possibly past the window's edge
	pub fn outer_rect(&self) -> Rect<f32> {
		self.rect().to_f32().outset(self.padding.map(|padding| padding as f32))
	}

	/// Where the cell at `row`, `col` is in [`Layer::cells`]
	pub fn index(&self, row: u32, col: u32) -> usize {
		row as usize * self.width as usize + col as usize
	}

	/// The cells, a row at a time
	pub fn rows(&self) -> std::slice::ChunksExact<'_, RenderCell> {
		self.cells.chunks_exact((self.width as usize).max(1))
	}
}

/// A line around a [`Layer`], in physical pixels
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
	/// Thickness, in physical pixels
	pub width: f32,

	/// As `0xRRGGBB` in sRGB
	pub color: u32,
}

/// A shadow under an area of the window, made of rectangles that may overlap
#[derive(Debug, Clone, PartialEq)]
pub struct LayerShadow {
	/// How the shadow looks
	pub shadow: Shadow,

	/// Rectangles in pixels from the window's top-left corner
	pub area: Vec<Rect<u32>>,

	/// Radius of the area's rectangles' rounded corners, in pixels, each at most its own size: windows' with rounded
	/// corners. The shadow follows them, and is drawn under the cut-off corners too.
	pub radius: f32,
}

/// A soft shadow under a rectangle, like CSS's `box-shadow`, in physical pixels
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
	/// How far the edge fades out, like CSS's blur radius: twice the Gaussian blur's standard deviation
	pub blur: f32,

	/// How far the shadow is moved from the rectangle: right and down, or left and up if negative
	pub offset: [f32; 2],

	/// As `0xRRGGBB` in sRGB
	pub color: u32,

	/// From 0, invisible, to 1, as dark as `color` where fully shadowed
	pub opacity: f32,
}

/// Text of a [`Layer`] that moved since the layer was last shown, e.g. a window that scrolled
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scroll {
	/// Rows the text moved up, or down if negative
	pub rows: i32,

	/// The cells that scroll, in cells from the layer's top-left corner: rows `top..bottom` and columns
	/// `left..right`. The rest, like a winbar or a border, stays in place.
	pub top: u32,
	/// Past the last row that scrolls
	pub bottom: u32,
	/// The first column that scrolls
	pub left: u32,
	/// Past the last column that scrolls
	pub right: u32,

	/// How the text slides to its new place, if not as the renderer's other scrolled text does (see
	/// [`crate::Renderer::set_scroll_motion`])
	pub motion: Option<Motion>,
}

/// A cell of a [`Layer`], with its highlight resolved to colors and styles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderCell {
	/// Empty for the right half of a double-width character
	pub text: ArrayString<32>,
	/// How its text is drawn
	pub style: TextStyle,

	/// As `0xRRGGBB` in sRGB
	pub background: u32,

	/// How much of what's under the cell shows through its background: from 0, none, to 100, all of it, like Neovim's
	/// `winblend`
	pub blend: u8,

	/// Lines drawn over its text
	pub decoration: Decoration,
}

/// Lines drawn over a cell's text
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Decoration {
	/// The line drawn under the text, if any
	pub underline: Option<Underline>,

	/// Drawn in the text color
	pub strikethrough: bool,

	/// Drawn in the text color
	pub overline: bool,

	/// Color of the underline, as `0xRRGGBB` in sRGB (Neovim's `special` color)
	pub underline_color: u32,
}

/// The style of an underline, like Neovim's `underline`, `underdouble`, `undercurl`, `underdotted` and `underdashed`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Underline {
	/// A straight line
	Single,
	/// Two straight lines
	Double,
	/// A wave
	Curly,
	/// A line of dots
	Dotted,
	/// A line of dashes
	Dashed,
}

/// How a cell's text is drawn
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextStyle {
	/// As `0xRRGGBB` in sRGB
	pub foreground: u32,
	/// Drawn in a bold face
	pub bold: bool,
	/// Drawn in an italic face
	pub italic: bool,
}

/// The cursor in a cell of a [`Layer`], drawn over the text
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
	/// Position of the cell, in cells from the layer's top-left corner
	pub row: u32,
	/// Column of the cell
	pub col: u32,

	/// Cells covered, 2 for a double-width character
	pub width: u32,

	/// What the cursor looks like
	pub shape: CursorShape,

	/// The cursor's color: a block's background, or a bar's color. As `0xRRGGBB` in sRGB.
	pub color: u32,

	/// Color of the character under a block cursor, so it stays readable. As `0xRRGGBB` in sRGB.
	pub text_color: u32,

	/// Whether moving here animates (see [`crate::CursorMotion`]); otherwise the cursor jumps here.
	pub animate: bool,
}

/// What a [`Cursor`] looks like, from Neovim's `guicursor`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
	/// The whole cell, with its character drawn again in the cursor's text color
	Block,

	/// A bar covering `percentage` percent of the cell
	Bar {
		/// Which edge of the cell it lies along
		orientation: BarOrientation,

		/// How much of the cell it covers, from 1 to 100
		percentage: u32,
	},

	/// The whole cell's outline, as thick as underlines, with its character in its own colors: e.g. while the window
	/// doesn't have the focus
	Outline,
}

/// Which edge of the cell a bar cursor lies along
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarOrientation {
	/// Along the left edge of the cell, e.g. in insert mode
	Vertical,

	/// Along the bottom edge of the cells, e.g. in replace mode
	Horizontal,
}
