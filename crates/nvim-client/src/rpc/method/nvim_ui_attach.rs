use serde::{Serialize, Serializer};

use crate::RpcMethod;

/// Neovim's [`nvim_ui_attach`](https://neovim.io/doc/user/api/#nvim_ui_attach()) function
pub struct NvimUiAttach;

impl RpcMethod for NvimUiAttach {
	const METHOD: &'static str = "nvim_ui_attach";

	type Params = NvimUiAttachParams;
	type Response = rmpv::Value;
}

#[derive(Debug)]
pub struct NvimUiAttachParams {
	pub width: u32,
	pub height: u32,
	pub options: NvimUiOptions,
}

/// As positional arguments, the way Neovim takes them
impl Serialize for NvimUiAttachParams {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		(self.width, self.height, &self.options).serialize(serializer)
	}
}

/// Neovim's external UI [`options`](https://neovim.io/doc/user/api-ui-events/#ui-option)
#[derive(Debug, Default, Serialize)]
pub struct NvimUiOptions {
	pub ext_multigrid: bool,
	pub ext_linegrid: bool,
	pub ext_hlstate: bool,
}

impl NvimUiOptions {
	pub fn all() -> Self {
		Self { ext_multigrid: true, ext_linegrid: true, ext_hlstate: true }
	}
}
