use serde::{Serialize, Serializer};

use crate::RpcMethod;

/// Neovim's [`nvim_ui_try_resize`](https://neovim.io/doc/user/api/#nvim_ui_try_resize()) function
pub struct UiTryResize;

impl RpcMethod for UiTryResize {
	const METHOD: &'static str = "nvim_ui_try_resize";

	type Params = UiTryResizeParams;
	type Response = rmpv::Value;
}

#[derive(Debug)]
pub struct UiTryResizeParams {
	pub width: u32,
	pub height: u32,
}

/// As positional arguments, the way Neovim takes them
impl Serialize for UiTryResizeParams {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		(self.width, self.height).serialize(serializer)
	}
}
