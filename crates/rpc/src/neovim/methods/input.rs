use crate::RpcMethod;

/// Neovim's [`nvim_input`](https://neovim.io/doc/user/api/#nvim_input()) function: queues keys as if typed, in
/// `:help key-notation` (a literal `<` is `<lt>`)
pub struct Input;

impl RpcMethod for Input {
	const METHOD: &'static str = "nvim_input";

	type Params = [String; 1];

	/// Number of bytes written
	type Response = u64;
}
