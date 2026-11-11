//! Mouse input for Neovim, sent like the TUI does: while Neovim has mouse input on (`mouse_on`), as presses, drags and
//! releases of buttons, wheel turns, and moves when the `mousemoveevent` option asks for them, each on a grid's cell.

use editor::{Editor, Pointer};
use renderer::CellSize;
use rpc::neovim::{InputMouseParams, MouseButton, MouseInput, WheelDirection};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton as WinitButton, MouseScrollDelta};
use winit::keyboard::ModifiersState;

/// Lines a wheel notch scrolls, the default of Neovim's `mousescroll`
const LINES_PER_NOTCH: f64 = 3.0;

/// What the mouse is doing over a window
#[derive(Debug)]
pub struct Mouse {
	/// Lines a touchpad scrolls per row of finger movement (see [`crate::settings::TouchpadSettings`])
	touchpad_scroll: f64,

	/// The pointer's position, in physical pixels from the window's top-left, while it's over the window
	position: Option<PhysicalPosition<f64>>,

	/// The button held, and the grid it was pressed on, which gets the drags and the release
	held: Option<(MouseButton, u32)>,

	/// The grid and cell of the last press, drag or move sent, so moving within a cell sends nothing
	last: Option<(u32, i64, i64)>,

	/// Wheel turns not sent yet, in notches: across (right is positive) and down (up is positive)
	wheel: (f64, f64),
}

impl Mouse {
	pub fn new(touchpad_scroll: f64) -> Self {
		Self { touchpad_scroll, position: None, held: None, last: None, wheel: (0.0, 0.0) }
	}

	pub fn set_touchpad_scroll(&mut self, touchpad_scroll: f64) {
		self.touchpad_scroll = touchpad_scroll;
	}

	/// The pointer moved to `position` in the window: a drag while a button is held, otherwise a move if Neovim asks
	/// for them
	pub fn moved(
		&mut self,
		position: PhysicalPosition<f64>,
		editor: &Editor,
		modifiers: ModifiersState,
	) -> Option<InputMouseParams> {
		self.position = Some(position);

		let (input, grid, row, col) = if let Some((button, grid)) = self.held {
			// A drag stays with the grid the press was on, even past its edges.
			let Some((row, col)) = editor.grid_cell(grid, position.x, position.y) else {
				self.held = None;
				return None;
			};
			(MouseInput::Drag(button), grid, row, col)
		} else if editor.mouse() && editor.mouse_move_events() {
			let hit = editor.grid_at(position.x, position.y)?;
			(MouseInput::Move, hit.grid, i64::from(hit.row), i64::from(hit.col))
		} else {
			return None;
		};

		if self.last == Some((grid, row, col)) {
			return None;
		}
		self.last = Some((grid, row, col));
		Some(InputMouseParams { input, modifiers: notation(modifiers), grid, row, col })
	}

	/// How the pointer should look where it is, or `None` to keep its look: while a button is held, e.g. dragging a
	/// separator, it stays as it was where the drag started.
	pub fn pointer(&self, editor: &Editor) -> Option<Pointer> {
		if self.held.is_some() {
			return None;
		}
		let position = self.position?;
		Some(editor.pointer_at(position.x, position.y))
	}

	/// The pointer left the window. While a button is held, its last position stays, so the release, which may come
	/// from outside the window, is still sent.
	pub fn left(&mut self) {
		if self.held.is_none() {
			self.position = None;
		}
	}

	/// `button` was pressed or released: a press on the grid under the pointer, a release on the grid it was pressed on
	pub fn button(
		&mut self,
		button: WinitButton,
		state: ElementState,
		editor: &Editor,
		modifiers: ModifiersState,
	) -> Option<InputMouseParams> {
		let button = match button {
			WinitButton::Left => MouseButton::Left,
			WinitButton::Right => MouseButton::Right,
			WinitButton::Middle => MouseButton::Middle,
			WinitButton::Back => MouseButton::X1,
			WinitButton::Forward => MouseButton::X2,
			WinitButton::Other(_) => return None,
		};
		let position = self.position?;

		match state {
			ElementState::Pressed => {
				if !editor.mouse() || self.held.is_some() {
					return None;
				}
				let hit = editor.grid_at(position.x, position.y)?;
				let (row, col) = (i64::from(hit.row), i64::from(hit.col));
				self.held = Some((button, hit.grid));
				self.last = Some((hit.grid, row, col));
				let input = MouseInput::Press(button);
				Some(InputMouseParams { input, modifiers: notation(modifiers), grid: hit.grid, row, col })
			},
			// The press was sent, so the release is too, even if mouse input went off since.
			ElementState::Released => {
				let (held, grid) = self.held.filter(|(held, _)| *held == button)?;
				self.held = None;
				let (row, col) = editor.grid_cell(grid, position.x, position.y)?;
				let input = MouseInput::Release(held);
				Some(InputMouseParams { input, modifiers: notation(modifiers), grid, row, col })
			},
		}
	}

	/// The wheel turned, or a touchpad scrolled, over the grid under the pointer: a wheel event per notch
	pub fn wheel(
		&mut self,
		delta: MouseScrollDelta,
		editor: &Editor,
		cell: CellSize,
		modifiers: ModifiersState,
	) -> Vec<InputMouseParams> {
		let (across, down) = match delta {
			MouseScrollDelta::LineDelta(across, down) => (f64::from(across), f64::from(down)),
			// A touchpad: rows (or columns) of movement, times the lines each scrolls, in notches
			MouseScrollDelta::PixelDelta(pixels) => {
				let notches = self.touchpad_scroll / LINES_PER_NOTCH;
				(pixels.x / f64::from(cell.width) * notches, pixels.y / f64::from(cell.height) * notches)
			},
		};
		self.wheel = (self.wheel.0 + across, self.wheel.1 + down);

		let Some(hit) = self.position.and_then(|position| editor.grid_at(position.x, position.y)) else {
			return Vec::new();
		};
		if !editor.mouse() {
			self.wheel = (0.0, 0.0);
			return Vec::new();
		}

		// Positive values move the content right and down, revealing what's left of and above it.
		let mut notches = Vec::new();
		let mut take = |turned: &mut f64, positive, negative| {
			while turned.abs() >= 1.0 {
				notches.push(if *turned > 0.0 { positive } else { negative });
				*turned -= turned.signum();
			}
		};
		take(&mut self.wheel.1, WheelDirection::Up, WheelDirection::Down);
		take(&mut self.wheel.0, WheelDirection::Left, WheelDirection::Right);

		let (row, col) = (i64::from(hit.row), i64::from(hit.col));
		let event = |direction| InputMouseParams {
			input: MouseInput::Wheel(direction),
			modifiers: notation(modifiers),
			grid: hit.grid,
			row,
			col,
		};
		notches.into_iter().map(event).collect()
	}
}

/// The modifiers held, as Neovim names them for mouse events: one letter each, e.g. "SC" for Shift+Ctrl
fn notation(modifiers: ModifiersState) -> String {
	[
		(modifiers.shift_key(), 'S'),
		(modifiers.control_key(), 'C'),
		(modifiers.alt_key(), 'M'),
		(modifiers.super_key(), 'D'),
	]
	.into_iter()
	.filter_map(|(held, letter)| held.then_some(letter))
	.collect()
}

#[cfg(test)]
mod tests {
	use editor::Editor;
	use renderer::CellSize;
	use rpc::neovim::{
		GridResizeEvent, InputMouseParams, MouseButton, MouseInput, OptionSetEvent, OptionValue, RedrawNotification,
		WheelDirection, WinExternalPosEvent, WinPosEvent, Window,
	};
	use serde::de::IgnoredAny;
	use winit::dpi::PhysicalPosition;
	use winit::event::{ElementState, MouseButton as WinitButton, MouseScrollDelta};
	use winit::keyboard::ModifiersState;

	use super::Mouse;

	const CELL: CellSize = CellSize { width: 8, height: 20 };

	/// A 20×10 screen with window grid 2 (10×5) at row 2, col 4, and mouse input on
	fn editor() -> Editor {
		let mut editor = Editor::new();
		editor.set_cell_size(CELL);
		let resize =
			|grid, width, height| RedrawNotification::GridResize(vec![GridResizeEvent { grid, width, height }]);
		let position = WinPosEvent { grid: 2, win: IgnoredAny, start_row: 2, start_col: 4, width: 10, height: 5 };
		editor.apply(vec![
			resize(1, 20, 10),
			resize(2, 10, 5),
			RedrawNotification::WinPos(vec![position]),
			RedrawNotification::MouseOn,
			RedrawNotification::Flush,
		]);
		editor
	}

	/// The middle of screen cell `row`, `col`
	fn at(row: f64, col: f64) -> PhysicalPosition<f64> {
		PhysicalPosition::new((col + 0.5) * 8.0, (row + 0.5) * 20.0)
	}

	fn event(input: MouseInput, grid: u32, row: i64, col: i64) -> Option<InputMouseParams> {
		Some(InputMouseParams { input, modifiers: String::new(), grid, row, col })
	}

	const NONE: ModifiersState = ModifiersState::empty();

	#[test]
	fn press_drag_and_release() {
		let (editor, mut mouse) = (editor(), Mouse::new(3.0));
		let left = MouseButton::Left;

		assert_eq!(mouse.moved(at(3.0, 5.0), &editor, NONE), None);
		let press = mouse.button(WinitButton::Left, ElementState::Pressed, &editor, NONE);
		assert_eq!(press, event(MouseInput::Press(left), 2, 1, 1));

		// Within the same cell: nothing; into the next row: a drag, also past the window's bottom
		assert_eq!(mouse.moved(PhysicalPosition::new(47.0, 79.0), &editor, NONE), None);
		assert_eq!(mouse.moved(at(4.0, 5.0), &editor, NONE), event(MouseInput::Drag(left), 2, 2, 1));
		assert_eq!(mouse.moved(at(9.0, 2.0), &editor, NONE), event(MouseInput::Drag(left), 2, 7, -2));

		// Released outside the window: still sent, where the pointer was last
		mouse.left();
		let release = mouse.button(WinitButton::Left, ElementState::Released, &editor, NONE);
		assert_eq!(release, event(MouseInput::Release(left), 2, 7, -2));
		// Afterwards, moving sends nothing: Neovim didn't ask for moves.
		assert_eq!(mouse.moved(at(0.0, 0.0), &editor, NONE), None);
	}

	#[test]
	fn an_external_window_gets_its_own_clicks() {
		let (mut editor, mut mouse) = (editor(), Mouse::new(3.0));
		// A 4×2 external window: 32×40 pixels, centered on the 160×200 pixel screen at (64, 80)
		let resize = RedrawNotification::GridResize(vec![GridResizeEvent { grid: 4, width: 4, height: 2 }]);
		let external = RedrawNotification::WinExternalPos(vec![WinExternalPosEvent { grid: 4, win: Window(1004) }]);
		editor.apply(vec![resize, external, RedrawNotification::Flush]);

		mouse.moved(PhysicalPosition::new(65.0, 81.0), &editor, NONE);
		let press = mouse.button(WinitButton::Left, ElementState::Pressed, &editor, NONE);
		assert_eq!(press, event(MouseInput::Press(MouseButton::Left), 4, 0, 0));
		// Dragged out of it, past its top-left corner, in its own cells
		let drag = mouse.moved(PhysicalPosition::new(50.0, 50.0), &editor, NONE);
		assert_eq!(drag, event(MouseInput::Drag(MouseButton::Left), 4, -2, -2));
	}

	#[test]
	fn modifiers_and_other_buttons() {
		let (editor, mut mouse) = (editor(), Mouse::new(3.0));
		mouse.moved(at(0.0, 0.0), &editor, NONE);
		let modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
		let press = mouse.button(WinitButton::Back, ElementState::Pressed, &editor, modifiers);
		let expected = InputMouseParams {
			input: MouseInput::Press(MouseButton::X1),
			modifiers: String::from("SC"),
			grid: 1,
			row: 0,
			col: 0,
		};
		assert_eq!(press, Some(expected));
	}

	#[test]
	fn nothing_while_mouse_input_is_off() {
		let (mut editor, mut mouse) = (editor(), Mouse::new(3.0));
		editor.apply(vec![RedrawNotification::MouseOff]);
		mouse.moved(at(3.0, 5.0), &editor, NONE);
		assert_eq!(mouse.button(WinitButton::Left, ElementState::Pressed, &editor, NONE), None);
		assert_eq!(mouse.wheel(MouseScrollDelta::LineDelta(0.0, -1.0), &editor, CELL, NONE), []);
		assert_eq!(mouse.button(WinitButton::Left, ElementState::Released, &editor, NONE), None);
	}

	#[test]
	fn moves_when_neovim_asks_for_them() {
		let (mut editor, mut mouse) = (editor(), Mouse::new(3.0));
		let option = OptionSetEvent { name: String::from("mousemoveevent"), value: OptionValue::Bool(true) };
		editor.apply(vec![RedrawNotification::OptionSet(vec![option])]);
		assert_eq!(mouse.moved(at(2.0, 4.0), &editor, NONE), event(MouseInput::Move, 2, 0, 0));
		assert_eq!(mouse.moved(at(2.0, 4.0), &editor, NONE), None);
		assert_eq!(mouse.moved(at(0.0, 0.0), &editor, NONE), event(MouseInput::Move, 1, 0, 0));
	}

	#[test]
	fn wheel_notches() {
		let (editor, mut mouse) = (editor(), Mouse::new(3.0));
		mouse.moved(at(3.0, 5.0), &editor, NONE);
		let down = || event(MouseInput::Wheel(WheelDirection::Down), 2, 1, 1).unwrap();

		assert_eq!(mouse.wheel(MouseScrollDelta::LineDelta(0.0, -2.0), &editor, CELL, NONE), [down(), down()]);
		// Half a notch waits for the other half.
		assert_eq!(mouse.wheel(MouseScrollDelta::LineDelta(0.0, 0.5), &editor, CELL, NONE), []);
		let up = event(MouseInput::Wheel(WheelDirection::Up), 2, 1, 1).unwrap();
		assert_eq!(mouse.wheel(MouseScrollDelta::LineDelta(0.0, 0.5), &editor, CELL, NONE), [up]);

		// A touchpad scrolling 3 lines per row of movement: a notch per row (20 px), and content moving right reveals
		// what's left of it
		let pixels = |x, y| MouseScrollDelta::PixelDelta(PhysicalPosition::new(x, y));
		assert_eq!(mouse.wheel(pixels(0.0, -19.0), &editor, CELL, NONE), []);
		assert_eq!(mouse.wheel(pixels(0.0, -1.0), &editor, CELL, NONE), [down()]);
		let left = event(MouseInput::Wheel(WheelDirection::Left), 2, 1, 1).unwrap();
		assert_eq!(mouse.wheel(pixels(8.0, 0.0), &editor, CELL, NONE), [left]);

		// At 1 line per row, the text follows the fingers: a notch per three rows
		let mut mouse = Mouse::new(1.0);
		mouse.moved(at(3.0, 5.0), &editor, NONE);
		assert_eq!(mouse.wheel(pixels(0.0, -59.0), &editor, CELL, NONE), []);
		assert_eq!(mouse.wheel(pixels(0.0, -1.0), &editor, CELL, NONE), [down()]);
	}
}
