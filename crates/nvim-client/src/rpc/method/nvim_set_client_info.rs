use std::collections::HashMap;

use serde::{Serialize, Serializer};

use crate::RpcMethod;

/// Neovim's [`nvim_set_client_info`](https://neovim.io/doc/user/api/#nvim_set_client_info()) function: identifies the
/// client on its channel, so Lua code can find it with `nvim_list_chans()` or `nvim_get_chan_info()`
pub struct NvimSetClientInfo;

impl RpcMethod for NvimSetClientInfo {
	const METHOD: &'static str = "nvim_set_client_info";

	type Params = NvimClientInfo;
	type Response = ();
}

#[derive(Debug)]
pub struct NvimClientInfo {
	pub name: String,
	pub version: NvimClientVersion,

	/// "remote", "msgpack-rpc", "ui", "embedder", "host" or "plugin"
	pub kind: &'static str,
}

#[derive(Debug, Serialize)]
pub struct NvimClientVersion {
	pub major: u32,
	pub minor: u32,
	pub patch: u32,
}

/// As positional arguments, the way Neovim takes them, without methods or attributes
impl Serialize for NvimClientInfo {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		let none = HashMap::<String, ()>::new();
		(&self.name, &self.version, self.kind, &none, &none).serialize(serializer)
	}
}
