//! The cursor's style from Neovim's modes (`:help ui-event-mode_info_set`), and how its moves animate.

use renderer::{CellSize, CursorMotion};
use rpc::neovim::{CursorShape, HlId, ModeInfo};
use rpc::sarthi::{CursorAnimationSettings, CursorMove, Mode};
use smart_default::SmartDefault;

/// How Neovim wants the cursor drawn, from `mode_info_set`, `mode_change`, `busy_start` and `busy_stop`
/// (`:help ui-event-mode_info_set`), and how it moves to where it's drawn next.
#[derive(Debug, SmartDefault)]
pub struct CursorState {
	/// Whether to use the modes' styles; otherwise the cursor is always a block.
	style_enabled: bool,
	modes: Vec<ModeInfo>,
	mode: usize,

	/// The mode's name, like `insert` or `cmdline_normal`, and the one the cursor was last drawn in.
	mode_name: String,
	drawn_mode: String,

	/// Where the cursor was last drawn: its cell's top-left corner, in pixels; none before it's drawn.
	drawn_at: Option<[u32; 2]>,

	/// Neovim asked not to draw the cursor.
	hidden: bool,

	/// How its moves animate.
	animation: CursorAnimationSettings,

	/// Whether the window has the focus.
	#[default(true)]
	focused: bool,
}

/// The cursor's look in the current mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorStyle {
	/// Block, vertical or horizontal bar.
	pub shape: CursorShape,

	/// How much of the cell a horizontal or vertical cursor covers, in percent.
	pub percentage: u32,

	/// Highlight for the cursor's colors; 0 means the cell's colors swapped.
	pub attr_id: HlId,

	/// Drawn as an outline, the cell's character in its own colors: while the window doesn't have the focus.
	pub outline: bool,
}

impl CursorState {
	/// Neovim's modes, from `mode_info_set`, and whether its cursor styles are used at all (`'guicursor'`).
	pub fn set_modes(&mut self, style_enabled: bool, modes: Vec<ModeInfo>) {
		self.style_enabled = style_enabled;
		self.modes = modes;
	}

	/// The current mode, by its index in the modes and its name (`mode_change`).
	pub fn set_mode(&mut self, mode: usize, name: &str) {
		self.mode = mode;
		name.clone_into(&mut self.mode_name);
	}

	/// How the cursor's moves animate, from its next move on.
	pub fn set_animation(&mut self, animation: CursorAnimationSettings) {
		self.animation = animation;
	}

	/// Whether the window has the focus. Without it, the cursor is an outline and jumps to where it moves.
	pub fn set_focused(&mut self, focused: bool) {
		self.focused = focused;
	}

	/// The cursor was drawn in the current mode, in a cell whose top-left corner is at `at`, in pixels.
	pub fn drawn(&mut self, at: [u32; 2]) {
		self.drawn_mode.clone_from(&self.mode_name);
		self.drawn_at = Some(at);
	}

	/// Hides the cursor while Neovim is busy (`busy_start`), or shows it again (`busy_stop`).
	pub fn set_hidden(&mut self, hidden: bool) {
		self.hidden = hidden;
	}

	/// The style to draw the cursor with, or `None` while it's hidden. Missing keys fall back to a block in the
	/// cell's swapped colors, which is also used before Neovim sends any modes.
	pub fn style(&self) -> Option<CursorStyle> {
		if self.hidden {
			return None;
		}

		let block =
			CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: HlId::DEFAULT, outline: !self.focused };
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

	/// How the cursor moves to a cell whose top-left corner is at `to`, in pixels, `cell` large: as the animation
	/// settings say for the move, or at once while they're off, without the focus, for its first place, and for a
	/// curve of `none`.
	pub fn motion_to(&self, to: [u32; 2], cell: CellSize) -> Option<CursorMotion> {
		let from = self.drawn_at.filter(|_| self.animation.enabled && self.focused)?;
		// In whole cells of where it goes, so a move to a window with other fonts counts its cells.
		let cells = |from: u32, to: u32, size: u32| (from.abs_diff(to) + size / 2) / size.max(1);
		let mode = mode_of(&self.mode_name).filter(|mode| mode_of(&self.drawn_mode) == Some(*mode));
		let step =
			CursorMove { rows: cells(from[1], to[1], cell.height), columns: cells(from[0], to[0], cell.width), mode };
		let step = self.animation.for_move(&step);
		Some(CursorMotion { motion: crate::motion(step.curve)?, trail: step.trail })
	}
}

/// The mode `vim.keymap.set()` names for a mode's name in `mode_change`, if it has one, e.g. [`Mode::Cmdline`] for
/// `cmdline_insert`.
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
	use renderer::CellSize;
	use rpc::neovim::{CursorShape, HlId, ModeInfo};
	use rpc::sarthi::{Animation, CursorAnimationSettings, CursorOverride, Mode, Modes, Span};

	use super::{CursorState, CursorStyle};

	const CELL: CellSize = CellSize { width: 10, height: 20 };

	const BLOCK: CursorStyle =
		CursorStyle { shape: CursorShape::Block, percentage: 100, attr_id: HlId(0), outline: false };

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

		// Missing keys fall back to the block.
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
	fn moves_take_the_values_of_the_overrides_that_apply() {
		let (spring, quick) =
			(Animation::Spring { stiffness: 700.0, damping: 53.0 }, Animation::EaseOut { duration: 40.0 });
		let mut cursor = CursorState::default();
		cursor.set_animation(CursorAnimationSettings {
			overrides: vec![
				CursorOverride {
					rows: Span::exactly(0),
					columns: Span { min: None, max: Some(2) },
					curve: Some(quick),
					trail: Some(0.0),
					..Default::default()
				},
				CursorOverride {
					modes: Some(Modes(vec![Mode::Insert, Mode::Cmdline])),
					curve: Some(Animation::None),
					..Default::default()
				},
			],
			..Default::default()
		});
		let at = |col: u32, row: u32| [col * CELL.width, row * CELL.height];
		let step = |cursor: &CursorState, col, row| {
			cursor.motion_to(at(col, row), CELL).map(|motion| (motion.motion, motion.trail))
		};
		let expected = |curve, trail| crate::motion(curve).map(|motion| (motion, trail));

		// Its first place: at once.
		assert_eq!(step(&cursor, 3, 0), None);
		cursor.set_mode(0, "normal");
		cursor.drawn(at(3, 0));
		// `l`, `2l`: quick and without a trail; 3 columns, or a row down: the base.
		assert_eq!(step(&cursor, 4, 0), expected(quick, 0.0));
		assert_eq!(step(&cursor, 1, 0), expected(quick, 0.0));
		assert_eq!(step(&cursor, 6, 0), expected(spring, 1.0));
		assert_eq!(step(&cursor, 3, 1), expected(spring, 1.0));

		// Into insert mode, e.g. with `A`: a move between modes, which insert mode's override isn't for.
		cursor.set_mode(1, "insert");
		assert_eq!(step(&cursor, 40, 0), expected(spring, 1.0));
		cursor.drawn(at(40, 0));
		// Typing in it, Replace mode too: both overrides apply, the later one's curve over the earlier one's, a jump.
		assert_eq!(step(&cursor, 41, 0), expected(Animation::None, 0.0));
		cursor.set_mode(1, "replace");
		assert_eq!(step(&cursor, 41, 0), expected(Animation::None, 0.0));
		// Out of it.
		cursor.set_mode(0, "normal");
		assert_eq!(step(&cursor, 41, 0), expected(quick, 0.0));
		cursor.drawn(at(41, 0));

		// The command line's modes are one: into it with `:`, then within it.
		cursor.set_mode(2, "cmdline_normal");
		assert_eq!(step(&cursor, 0, 30), expected(spring, 1.0));
		cursor.drawn(at(0, 30));
		cursor.set_mode(2, "cmdline_insert");
		assert_eq!(step(&cursor, 5, 30), expected(Animation::None, 1.0));
		// Modes `vim.keymap.set()` has no name for are no override's.
		cursor.set_mode(0, "more");
		cursor.drawn(at(5, 30));
		assert_eq!(step(&cursor, 5, 31), expected(spring, 1.0));
	}

	#[test]
	fn moves_jump_while_animation_is_off_or_without_the_focus() {
		let mut cursor = CursorState::default();
		cursor.drawn([0, 0]);
		assert!(cursor.motion_to([100, 0], CELL).is_some());
		cursor.set_focused(false);
		assert_eq!(cursor.motion_to([100, 0], CELL), None);
		cursor.set_focused(true);
		cursor.set_animation(CursorAnimationSettings { enabled: false, ..Default::default() });
		assert_eq!(cursor.motion_to([100, 0], CELL), None);
	}

	#[test]
	fn an_outline_without_the_focus() {
		let mut cursor = CursorState::default();
		cursor.set_focused(false);
		assert_eq!(cursor.style(), Some(CursorStyle { outline: true, ..BLOCK }));
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
