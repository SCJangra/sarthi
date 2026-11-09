use nvim::{CursorShape, ModeInfo};

/// How Neovim wants the cursor drawn, from `mode_info_set`, `mode_change`, `busy_start` and `busy_stop`
/// (`:help ui-event-mode_info_set`)
#[derive(Debug, Default)]
pub struct CursorState {
	/// Whether to use the modes' styles; otherwise the cursor is always a block
	style_enabled: bool,
	modes: Vec<ModeInfo>,
	mode: usize,

	/// Neovim asked not to draw the cursor
	hidden: bool,
}

/// The cursor's look in the current mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorStyle {
	pub shape: CursorShape,

	/// How much of the cell a horizontal or vertical cursor covers, in percent
	pub percentage: u32,

	/// Highlight for the cursor's colors; 0 means the cell's colors swapped
	pub attr_id: u32,
}

impl CursorState {
	pub fn set_modes(&mut self, style_enabled: bool, modes: Vec<ModeInfo>) {
		self.style_enabled = style_enabled;
		self.modes = modes;
	}

	pub fn set_mode(&mut self, mode: usize) {
		self.mode = mode;
	}

	pub fn set_hidden(&mut self, hidden: bool) {
		self.hidden = hidden;
	}

	/// The style to draw the cursor with, or `None` while it's hidden. Missing keys fall back to a block in the
	/// cell's swapped colors, which is also used before Neovim sends any modes.
	pub fn style(&self) -> Option<CursorStyle> {
		if self.hidden {
			return None;
		}

		let block = CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: 0 };
		let Some(mode) = self.modes.get(self.mode).filter(|_| self.style_enabled) else {
			return Some(block);
		};

		Some(CursorStyle {
			shape: mode.cursor_shape.unwrap_or(block.shape),
			percentage: mode.cell_percentage.unwrap_or(block.percentage),
			attr_id: mode.attr_id.unwrap_or(block.attr_id),
		})
	}
}

#[cfg(test)]
mod tests {
	use nvim::{CursorShape, ModeInfo};

	use super::{CursorState, CursorStyle};

	const BLOCK: CursorStyle = CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: 0 };

	fn modes() -> Vec<ModeInfo> {
		vec![
			ModeInfo { cursor_shape: Some(CursorShape::Block), cell_percentage: Some(0), attr_id: Some(7) },
			ModeInfo { cursor_shape: Some(CursorShape::Vertical), cell_percentage: Some(25), attr_id: None },
			ModeInfo { cursor_shape: None, cell_percentage: None, attr_id: None },
		]
	}

	#[test]
	fn block_before_any_modes() {
		assert_eq!(CursorState::default().style(), Some(BLOCK));
	}

	#[test]
	fn style_of_the_current_mode() {
		let mut cursor = CursorState::default();
		cursor.set_modes(true, modes());

		assert_eq!(cursor.style(), Some(CursorStyle { shape: CursorShape::Block, percentage: 0, attr_id: 7 }));

		cursor.set_mode(1);
		assert_eq!(cursor.style(), Some(CursorStyle { shape: CursorShape::Vertical, percentage: 25, attr_id: 0 }));

		// Missing keys fall back to the block
		cursor.set_mode(2);
		assert_eq!(cursor.style(), Some(BLOCK));

		cursor.set_mode(9);
		assert_eq!(cursor.style(), Some(BLOCK));
	}

	#[test]
	fn block_when_styles_are_disabled() {
		let mut cursor = CursorState::default();
		cursor.set_modes(false, modes());
		cursor.set_mode(1);
		assert_eq!(cursor.style(), Some(BLOCK));
	}

	#[test]
	fn hidden_while_busy() {
		let mut cursor = CursorState::default();
		cursor.set_hidden(true);
		assert_eq!(cursor.style(), None);
		cursor.set_hidden(false);
		assert_eq!(cursor.style(), Some(BLOCK));
	}
}
