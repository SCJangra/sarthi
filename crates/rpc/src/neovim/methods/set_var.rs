use std::marker::PhantomData;

use serde::Serialize;

use crate::RpcMethod;

/// Neovim's [`nvim_set_var`](https://neovim.io/doc/user/api/#nvim_set_var()) function: sets the global variable
/// `g:name` (`vim.g.name` in Lua) to a value of type `V`
pub struct SetVar<V>(PhantomData<V>);

impl<V: Serialize> RpcMethod for SetVar<V> {
	const METHOD: &'static str = "nvim_set_var";

	type Params = (String, V);
	type Response = ();
}
