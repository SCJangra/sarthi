use rpc::neovim::{CursorShape, ModeInfo};

/// How Neovim wants the cursor drawn, from `mode_info_set`, `mode_change`, `busy_start` and `busy_stop`
/// (`:help ui-event-mode_info_set`)
#[derive(Debug)]
pub struct CursorState {
	/// Whether to use the modes' styles; otherwise the cursor is always a block
	style_enabled: bool,
	modes: Vec<ModeInfo>,
	mode: usize,

	/// The mode's name, like `insert` or `cmdline_normal`, and the one the cursor was last drawn in
	mode_name: String,
	drawn_mode: String,

	/// Neovim asked not to draw the cursor
	hidden: bool,

	/// Whether moves animate in insert and replace mode, and into and out of the command line
	animate_insert_mode: bool,
	animate_cmdline: bool,

	/// Whether the window has the focus
	focused: bool,
}

impl Default for CursorState {
	fn default() -> Self {
		Self {
			style_enabled: false,
			modes: Vec::new(),
			mode: 0,
			mode_name: String::new(),
			drawn_mode: String::new(),
			hidden: false,
			animate_insert_mode: true,
			animate_cmdline: true,
			focused: true,
		}
	}
}

/// The cursor's look in the current mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorStyle {
	pub shape: CursorShape,

	/// How much of the cell a horizontal or vertical cursor covers, in percent
	pub percentage: u32,

	/// Highlight for the cursor's colors; 0 means the cell's colors swapped
	pub attr_id: u32,

	/// Whether moving here animates
	pub animate: bool,

	/// Drawn as an outline, the cell's character in its own colors: while the window doesn't have the focus
	pub outline: bool,
}

impl CursorState {
	pub fn set_modes(&mut self, style_enabled: bool, modes: Vec<ModeInfo>) {
		self.style_enabled = style_enabled;
		self.modes = modes;
	}

	pub fn set_mode(&mut self, mode: usize, name: &str) {
		self.mode = mode;
		name.clone_into(&mut self.mode_name);
	}

	/// Whether moves animate in insert and replace mode, and into and out of the command line
	pub fn set_animation(&mut self, insert_mode: bool, cmdline: bool) {
		(self.animate_insert_mode, self.animate_cmdline) = (insert_mode, cmdline);
	}

	/// Whether the window has the focus. Without it, the cursor is an outline and jumps to where it moves.
	pub fn set_focused(&mut self, focused: bool) {
		self.focused = focused;
	}

	/// The cursor was drawn in the current mode.
	pub fn drawn(&mut self) {
		self.drawn_mode.clone_from(&self.mode_name);
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

		let (animate, outline) = (self.focused && self.animates(), !self.focused);
		let block = CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: 0, animate, outline };
		let Some(mode) = self.modes.get(self.mode).filter(|_| self.style_enabled) else {
			return Some(block);
		};

		Some(CursorStyle {
			shape: mode.cursor_shape.unwrap_or(block.shape),
			percentage: mode.cell_percentage.unwrap_or(block.percentage),
			attr_id: mode.attr_id.unwrap_or(block.attr_id),
			..block
		})
	}

	/// Whether a move to the cursor animates: not in a mode, or into or out of one, that animation is off for
	fn animates(&self) -> bool {
		let typing = |mode: &str| mode == "insert" || mode == "replace";
		let cmdline = |mode: &str| mode.starts_with("cmdline");
		let off = |mode: &str| (typing(mode) && !self.animate_insert_mode) || (cmdline(mode) && !self.animate_cmdline);
		!off(&self.mode_name) && !(cmdline(&self.drawn_mode) && !self.animate_cmdline)
	}
}

#[cfg(test)]
mod tests {
	use rpc::neovim::{CursorShape, ModeInfo};

	use super::{CursorState, CursorStyle};

	const BLOCK: CursorStyle =
		CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: 0, animate: true, outline: false };

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

		assert_eq!(cursor.style(), Some(CursorStyle { shape: CursorShape::Block, percentage: 0, attr_id: 7, ..BLOCK }));

		cursor.set_mode(1, "insert");
		assert_eq!(cursor.style(), Some(CursorStyle { shape: CursorShape::Vertical, percentage: 25, ..BLOCK }));

		// Missing keys fall back to the block
		cursor.set_mode(2, "visual");
		assert_eq!(cursor.style(), Some(BLOCK));

		cursor.set_mode(9, "normal");
		assert_eq!(cursor.style(), Some(BLOCK));
	}

	#[test]
	fn block_when_styles_are_disabled() {
		let mut cursor = CursorState::default();
		cursor.set_modes(false, modes());
		cursor.set_mode(1, "insert");
		assert_eq!(cursor.style(), Some(BLOCK));
	}

	#[test]
	fn moves_animate_unless_turned_off_for_the_mode() {
		let animates = |cursor: &CursorState| cursor.style().unwrap().animate;
		let mut cursor = CursorState::default();
		cursor.set_mode(0, "normal");
		assert!(animates(&cursor));
		cursor.set_mode(1, "insert");
		assert!(animates(&cursor));

		cursor.set_animation(false, true);
		assert!(!animates(&cursor));
		cursor.set_mode(1, "replace");
		assert!(!animates(&cursor));

		// Into the command line, and back out of it
		cursor.set_animation(true, false);
		cursor.set_mode(0, "normal");
		cursor.drawn();
		cursor.set_mode(2, "cmdline_normal");
		assert!(!animates(&cursor));
		cursor.drawn();
		cursor.set_mode(0, "normal");
		assert!(!animates(&cursor));
		// Once drawn outside it, moves animate again.
		cursor.drawn();
		assert!(animates(&cursor));
	}

	#[test]
	fn an_outline_without_the_focus() {
		let mut cursor = CursorState::default();
		cursor.set_focused(false);
		// An outline, which jumps instead of moving
		assert_eq!(cursor.style(), Some(CursorStyle { animate: false, outline: true, ..BLOCK }));
		cursor.set_focused(true);
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
