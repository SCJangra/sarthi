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

	/// A cursor drawn over the text as a bar. A block cursor is drawn by giving the cells under it the cursor's
	/// colors instead.
	pub cursor: Option<BarCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderCell {
	/// Empty for the right half of a double-width character
	pub text: ArrayString<32>,
	pub style: TextStyle,

	/// As `0xRRGGBB` in sRGB
	pub background: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextStyle {
	/// As `0xRRGGBB` in sRGB
	pub foreground: u32,
	pub bold: bool,
	pub italic: bool,
}

/// A thin cursor in a cell of a [`CellLayer`], drawn over the text
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarCursor {
	/// Position of the cell, in cells from the layer's top-left corner
	pub row: u32,
	pub col: u32,

	/// Cells covered, 2 for a double-width character
	pub width: u32,

	pub orientation: BarOrientation,

	/// How much of the cell the bar covers, in percent
	pub percentage: u32,

	/// As `0xRRGGBB` in sRGB
	pub color: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarOrientation {
	/// Along the left edge of the cell, e.g. in insert mode
	Vertical,

	/// Along the bottom edge of the cells, e.g. in replace mode
	Horizontal,
}
