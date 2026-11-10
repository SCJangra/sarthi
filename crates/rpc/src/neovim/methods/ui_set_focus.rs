use crate::RpcMethod;

/// Neovim's [`nvim_ui_set_focus`](https://neovim.io/doc/user/api/#nvim_ui_set_focus()) function: tells Neovim whether
/// the UI has focus, which triggers `FocusGained` and `FocusLost`
pub struct UiSetFocus;

impl RpcMethod for UiSetFocus {
	const METHOD: &'static str = "nvim_ui_set_focus";

	type Params = [bool; 1];
	type Response = ();
}
