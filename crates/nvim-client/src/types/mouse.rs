use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
	Block,
	Horizontal,
	Vertical,
}

/// How the cursor looks in one mode (`:help ui-event-mode_info_set`). Some keys are missing in some modes.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ModeInfo {
	pub cursor_shape: Option<CursorShape>,

	/// How much of the cell a horizontal or vertical cursor covers, in percent
	pub cell_percentage: Option<u32>,

	/// Highlight for the cursor's colors; 0 means the cell's colors swapped
	pub attr_id: Option<u32>,
}
