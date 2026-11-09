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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderCell {
	/// Empty for the right half of a double-width character
	pub text: ArrayString<32>,
	pub style: TextStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextStyle {
	/// As `0xRRGGBB` in sRGB
	pub foreground: u32,
	pub bold: bool,
	pub italic: bool,
}
