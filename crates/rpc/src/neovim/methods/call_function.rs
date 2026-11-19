use std::marker::PhantomData;

use serde::de::DeserializeOwned;

use crate::RpcMethod;

/// [`nvim_call_function`](https://neovim.io/doc/user/api/#nvim_call_function()): calls a Vimscript function, e.g.
/// `winline()`, returning a value of type `R`
pub struct CallFunction<R>(PhantomData<R>);

impl<R: DeserializeOwned> RpcMethod for CallFunction<R> {
	const METHOD: &'static str = "nvim_call_function";
	type Params = (String, Vec<rmpv::Value>);
	type Response = R;
}
