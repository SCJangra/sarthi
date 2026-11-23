use crate::{NoOpts, RpcMethod};

/// [`nvim_get_option_value`](https://neovim.io/doc/user/api/#nvim_get_option_value()), of a string option.
pub struct GetStringOption;

impl RpcMethod for GetStringOption {
	const METHOD: &'static str = "nvim_get_option_value";
	type Params = (String, NoOpts);
	type Response = String;
}

/// [`nvim_set_option_value`](https://neovim.io/doc/user/api/#nvim_set_option_value()), of a string option.
pub struct SetStringOption;

impl RpcMethod for SetStringOption {
	const METHOD: &'static str = "nvim_set_option_value";
	type Params = (String, String, NoOpts);
	type Response = ();
}
