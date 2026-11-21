//! The cursor's style from Neovim's modes (`:help ui-event-mode_info_set`), and when its moves animate

use rpc::neovim::{CursorShape, HlId, ModeInfo};
use rpc::sarthi::Mode;
use smart_default::SmartDefault;

/// How Neovim wants the cursor drawn, from `mode_info_set`, `mode_change`, `busy_start` and `busy_stop`
/// (`:help ui-event-mode_info_set`)
#[derive(Debug, SmartDefault)]
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

	/// Whether moves animate at all, and the modes moves within which don't
	#[default(true)]
	animation_enabled: bool,
	exclude_modes: Vec<Mode>,

	/// Whether the window has the focus
	#[default(true)]
	focused: bool,
}

/// The cursor's look in the current mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorStyle {
	/// Block, vertical or horizontal bar
	pub shape: CursorShape,

	/// How much of the cell a horizontal or vertical cursor covers, in percent
	pub percentage: u32,

	/// Highlight for the cursor's colors; 0 means the cell's colors swapped
	pub attr_id: HlId,

	/// Whether moving here animates
	pub animate: bool,

	/// Drawn as an outline, the cell's character in its own colors: while the window doesn't have the focus
	pub outline: bool,
}

impl CursorState {
	/// Neovim's modes, from `mode_info_set`, and whether its cursor styles are used at all (`'guicursor'`)
	pub fn set_modes(&mut self, style_enabled: bool, modes: Vec<ModeInfo>) {
		self.style_enabled = style_enabled;
		self.modes = modes;
	}

	/// The current mode, by its index in the modes and its name (`mode_change`)
	pub fn set_mode(&mut self, mode: usize, name: &str) {
		self.mode = mode;
		name.clone_into(&mut self.mode_name);
	}

	/// Whether moves animate at all, and the modes moves within which don't: moves into and out of them still do
	pub fn set_animation(&mut self, enabled: bool, exclude_modes: Vec<Mode>) {
		(self.animation_enabled, self.exclude_modes) = (enabled, exclude_modes);
	}

	/// Whether the window has the focus. Without it, the cursor is an outline and jumps to where it moves.
	pub fn set_focused(&mut self, focused: bool) {
		self.focused = focused;
	}

	/// The cursor was drawn in the current mode.
	pub fn drawn(&mut self) {
		self.drawn_mode.clone_from(&self.mode_name);
	}

	/// Hides the cursor while Neovim is busy (`busy_start`), or shows it again (`busy_stop`)
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
		let block =
			CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: HlId::DEFAULT, animate, outline };
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

	/// Whether a move to the cursor animates: if animation is on, unless it's within a mode it's off for. A move into
	/// or out of one, drawn first in another mode than the last, animates.
	fn animates(&self) -> bool {
		let mode = mode_of(&self.mode_name);
		let changed = mode != mode_of(&self.drawn_mode);
		self.animation_enabled && (changed || mode.is_none_or(|mode| !self.exclude_modes.contains(&mode)))
	}
}

/// The mode `vim.keymap.set()` names for a mode's name in `mode_change`, if it has one, e.g. [`Mode::Cmdline`] for
/// `cmdline_insert`
fn mode_of(name: &str) -> Option<Mode> {
	match name {
		"normal" => Some(Mode::Normal),
		"visual" | "visual_select" => Some(Mode::Visual),
		"operator" => Some(Mode::OperatorPending),
		"insert" | "replace" => Some(Mode::Insert),
		"cmdline_normal" | "cmdline_insert" | "cmdline_replace" => Some(Mode::Cmdline),
		"terminal" => Some(Mode::Terminal),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use rpc::neovim::{CursorShape, HlId, ModeInfo};
	use rpc::sarthi::Mode;

	use super::{CursorState, CursorStyle};

	const BLOCK: CursorStyle =
		CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: HlId(0), animate: true, outline: false };

	fn modes() -> Vec<ModeInfo> {
		vec![
			ModeInfo { cursor_shape: Some(CursorShape::Block), cell_percentage: Some(0), attr_id: Some(HlId(7)) },
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

		assert_eq!(
			cursor.style(),
			Some(CursorStyle { shape: CursorShape::Block, percentage: 0, attr_id: HlId(7), ..BLOCK })
		);

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
	fn moves_animate_unless_within_an_excluded_mode() {
		let animates = |cursor: &CursorState| cursor.style().unwrap().animate;
		let mut cursor = CursorState::default();
		cursor.set_mode(0, "normal");
		cursor.drawn();
		assert!(animates(&cursor));
		cursor.set_mode(1, "insert");
		assert!(animates(&cursor));

		cursor.set_animation(true, vec![Mode::Insert, Mode::Cmdline]);
		// Into insert mode, e.g. with `A`, then typing in it, Replace mode too
		assert!(animates(&cursor));
		cursor.drawn();
		assert!(!animates(&cursor));
		cursor.set_mode(1, "replace");
		assert!(!animates(&cursor));
		// Out of it
		cursor.set_mode(0, "normal");
		assert!(animates(&cursor));
		cursor.drawn();

		// Into the command line with `:`, typing in it, whichever of its modes, and back out of it
		cursor.set_mode(2, "cmdline_normal");
		assert!(animates(&cursor));
		cursor.drawn();
		cursor.set_mode(2, "cmdline_insert");
		assert!(!animates(&cursor));
		cursor.set_mode(0, "normal");
		assert!(animates(&cursor));
		// Modes `vim.keymap.set()` has no name for animate.
		cursor.set_mode(0, "more");
		cursor.drawn();
		assert!(animates(&cursor));

		// Off: no move animates.
		cursor.set_animation(false, Vec::new());
		cursor.set_mode(1, "insert");
		assert!(!animates(&cursor));
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
