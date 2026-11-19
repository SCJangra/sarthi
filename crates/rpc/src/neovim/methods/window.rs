use serde::Serialize;

use crate::{Buffer, RpcMethod, Window};

/// [`nvim_get_current_win`](https://neovim.io/doc/user/api/#nvim_get_current_win())
pub struct GetCurrentWin;

impl RpcMethod for GetCurrentWin {
	const METHOD: &'static str = "nvim_get_current_win";
	type Params = [u8; 0];
	type Response = Window;
}

/// [`nvim_open_win`](https://neovim.io/doc/user/api/#nvim_open_win()), with its config as given
pub struct OpenWin;

impl RpcMethod for OpenWin {
	const METHOD: &'static str = "nvim_open_win";
	type Params = (Buffer, bool, rmpv::Value);
	type Response = Window;
}

/// [`nvim_win_close`](https://neovim.io/doc/user/api/#nvim_win_close())
pub struct WinClose;

impl RpcMethod for WinClose {
	const METHOD: &'static str = "nvim_win_close";
	type Params = (Window, bool);
	type Response = ();
}

/// [`nvim_win_get_config`](https://neovim.io/doc/user/api/#nvim_win_get_config())
pub struct WinGetConfig;

impl RpcMethod for WinGetConfig {
	const METHOD: &'static str = "nvim_win_get_config";
	type Params = (Window,);
	type Response = rmpv::Value;
}

/// [`nvim_win_set_config`](https://neovim.io/doc/user/api/#nvim_win_set_config())
pub struct WinSetConfig;

impl RpcMethod for WinSetConfig {
	const METHOD: &'static str = "nvim_win_set_config";
	type Params = (Window, rmpv::Value);
	type Response = ();
}

/// [`nvim_win_set_var`](https://neovim.io/doc/user/api/#nvim_win_set_var())
pub struct WinSetVar;

impl RpcMethod for WinSetVar {
	const METHOD: &'static str = "nvim_win_set_var";
	type Params = (Window, String, rmpv::Value);
	type Response = ();
}

/// No options, for the API functions that take an `opts` dictionary
#[derive(Debug, Default, Serialize)]
pub struct NoOpts {}
