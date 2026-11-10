use serde::{Serialize, Serializer};

use crate::RpcMethod;

/// Neovim's [`nvim_input_mouse`](https://neovim.io/doc/user/api/#nvim_input_mouse()) function: a mouse event, on a
/// cell of a grid
pub struct InputMouse;

impl RpcMethod for InputMouse {
	const METHOD: &'static str = "nvim_input_mouse";

	type Params = InputMouseParams;
	type Response = ();
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputMouseParams {
	pub input: MouseInput,

	/// Modifier keys held, in key notation without dashes, e.g. "CS" for Ctrl+Shift
	pub modifiers: String,

	/// The grid and its cell. A drag can go past the grid's edges, e.g. to select beyond the window, which scrolls it.
	pub grid: u32,
	pub row: i64,
	pub col: i64,
}

/// What the mouse did
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseInput {
	Press(MouseButton),
	Drag(MouseButton),
	Release(MouseButton),
	Wheel(WheelDirection),

	/// Moved with no button held, reported to Neovim when the `mousemoveevent` option is set
	Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
	Left,
	Right,
	Middle,

	/// The "back" and "forward" side buttons
	X1,
	X2,
}

/// Which way the content scrolls: a wheel turned down shows text further down
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelDirection {
	Up,
	Down,
	Left,
	Right,
}

impl MouseInput {
	/// Neovim's names for the button and the action
	fn names(self) -> (&'static str, &'static str) {
		let button = |button| match button {
			MouseButton::Left => "left",
			MouseButton::Right => "right",
			MouseButton::Middle => "middle",
			MouseButton::X1 => "x1",
			MouseButton::X2 => "x2",
		};
		match self {
			Self::Press(pressed) => (button(pressed), "press"),
			Self::Drag(dragged) => (button(dragged), "drag"),
			Self::Release(released) => (button(released), "release"),
			Self::Wheel(WheelDirection::Up) => ("wheel", "up"),
			Self::Wheel(WheelDirection::Down) => ("wheel", "down"),
			Self::Wheel(WheelDirection::Left) => ("wheel", "left"),
			Self::Wheel(WheelDirection::Right) => ("wheel", "right"),
			// Neovim ignores the action of a move.
			Self::Move => ("move", ""),
		}
	}
}

/// As positional arguments, the way Neovim takes them
impl Serialize for InputMouseParams {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		let (button, action) = self.input.names();
		(button, action, &self.modifiers, self.grid, self.row, self.col).serialize(serializer)
	}
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{InputMouseParams, MouseButton, MouseInput, WheelDirection};

	fn encode(input: MouseInput) -> Value {
		let params = InputMouseParams { input, modifiers: String::from("C"), grid: 2, row: -1, col: 4 };
		rmpv::ext::to_value(params).unwrap()
	}

	#[test]
	fn ser_input_mouse() {
		assert_eq!(encode(MouseInput::Press(MouseButton::Left)).to_string(), r#"["left", "press", "C", 2, -1, 4]"#);
		assert_eq!(encode(MouseInput::Release(MouseButton::X2)).to_string(), r#"["x2", "release", "C", 2, -1, 4]"#);
		assert_eq!(encode(MouseInput::Wheel(WheelDirection::Down)).to_string(), r#"["wheel", "down", "C", 2, -1, 4]"#);
		assert_eq!(encode(MouseInput::Move).to_string(), r#"["move", "", "C", 2, -1, 4]"#);
	}
}
