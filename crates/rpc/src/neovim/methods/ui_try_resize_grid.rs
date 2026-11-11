use serde::{Serialize, Serializer};

use crate::RpcMethod;

/// Neovim's [`nvim_ui_try_resize_grid`](https://neovim.io/doc/user/api/#nvim_ui_try_resize_grid()) function: resizes the
/// window of a grid, e.g. an external window the UI sized
pub struct UiTryResizeGrid;

impl RpcMethod for UiTryResizeGrid {
	const METHOD: &'static str = "nvim_ui_try_resize_grid";

	type Params = UiTryResizeGridParams;
	type Response = rmpv::Value;
}

#[derive(Debug)]
pub struct UiTryResizeGridParams {
	/// The grid, not its window's handle
	pub grid: u32,
	pub width: u32,
	pub height: u32,
}

/// As positional arguments, the way Neovim takes them
impl Serialize for UiTryResizeGridParams {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		(self.grid, self.width, self.height).serialize(serializer)
	}
}

#[cfg(test)]
mod tests {
	use super::UiTryResizeGridParams;

	#[test]
	fn positional_arguments() {
		let params = UiTryResizeGridParams { grid: 4, width: 30, height: 8 };
		// [4, 30, 8]
		assert_eq!(rmp_serde::to_vec(&params).unwrap(), [0x93, 0x04, 0x1E, 0x08]);
	}
}
