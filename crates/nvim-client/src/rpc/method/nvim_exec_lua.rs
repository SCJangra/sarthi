use crate::RpcMethod;

/// Neovim's [`nvim_exec_lua`](https://neovim.io/doc/user/api/#nvim_exec_lua()) function: runs Lua code with arguments
/// (`...` in the code) and returns its result
pub struct NvimExecLua;

impl RpcMethod for NvimExecLua {
	const METHOD: &'static str = "nvim_exec_lua";

	type Params = (String, Vec<rmpv::Value>);
	type Response = rmpv::Value;
}
