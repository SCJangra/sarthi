use arrayvec::ArrayString;

/// Something to draw in a window. Layers are drawn in order, so later ones cover earlier ones.
#[derive(Debug, Clone, PartialEq)]
pub enum Layer {
	Cells(CellLayer),
}

/// A block of cells at a cell position, e.g. one of Neovim's grids
#[derive(Debug, Clone, PartialEq)]
pub struct CellLayer {
	/// Identifies the layer across frames, so unchanged rows aren't laid out again
	pub id: u64,

	/// Position of the top-left cell, in cells from the window's top-left corner
	pub row: u32,
	pub col: u32,

	pub width: u32,
	pub height: u32,

	/// Row by row, `width * height` cells
	pub cells: Vec<RenderCell>,

	/// The cursor, drawn over the text where the layer's cells are, even while scrolled text slides
	pub cursor: Option<Cursor>,

	/// How the text moved since the layer was last shown, so the renderer can slide it to its new place
	pub scroll: Option<Scroll>,
}

/// Text of a [`CellLayer`] that moved since the layer was last shown, e.g. a window that scrolled
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scroll {
	/// Rows the text moved up, or down if negative
	pub rows: i32,

	/// The cells that scroll, in cells from the layer's top-left corner: rows `top..bottom` and columns
	/// `left..right`. The rest, like a winbar or a border, stays in place.
	pub top: u32,
	pub bottom: u32,
	pub left: u32,
	pub right: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderCell {
	/// Empty for the right half of a double-width character
	pub text: ArrayString<32>,
	pub style: TextStyle,

	/// As `0xRRGGBB` in sRGB
	pub background: u32,

	pub decoration: Decoration,
}

/// Lines drawn over a cell's text
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Decoration {
	pub underline: Option<Underline>,

	/// Drawn in the text color
	pub strikethrough: bool,

	/// Drawn in the text color
	pub overline: bool,

	/// Color of the underline, as `0xRRGGBB` in sRGB (Neovim's `special` color)
	pub underline_color: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Underline {
	Single,
	Double,
	Curly,
	Dotted,
	Dashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextStyle {
	/// As `0xRRGGBB` in sRGB
	pub foreground: u32,
	pub bold: bool,
	pub italic: bool,
}

/// The cursor in a cell of a [`CellLayer`], drawn over the text
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
	/// Position of the cell, in cells from the layer's top-left corner
	pub row: u32,
	pub col: u32,

	/// Cells covered, 2 for a double-width character
	pub width: u32,

	pub shape: CursorShape,

	/// The cursor's color: a block's background, or a bar's color. As `0xRRGGBB` in sRGB.
	pub color: u32,

	/// Color of the character under a block cursor, so it stays readable. As `0xRRGGBB` in sRGB.
	pub text_color: u32,

	/// Whether moving here animates (see [`crate::CursorMotion`]); otherwise the cursor jumps here.
	pub animate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
	/// The whole cell, with its character drawn again in the cursor's text color
	Block,

	/// A bar covering `percentage` percent of the cell
	Bar { orientation: BarOrientation, percentage: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarOrientation {
	/// Along the left edge of the cell, e.g. in insert mode
	Vertical,

	/// Along the bottom edge of the cells, e.g. in replace mode
	Horizontal,
}
