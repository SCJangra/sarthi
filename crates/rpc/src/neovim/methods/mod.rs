//! The API functions sarthi calls (`:help api-global`), one module each, with their parameters as Neovim takes them.

#![allow(missing_docs, reason = "named and typed as in Neovim's help, which documents them")]

mod call_function;
mod eval;
mod exec_lua;
mod get_chan_info;
mod input;
mod input_mouse;
mod option_value;
mod set_client_info;
mod set_var;
mod ui_attach;
mod ui_set_focus;
mod ui_try_resize;
mod ui_try_resize_grid;
mod window;

pub use call_function::*;
pub use eval::*;
pub use exec_lua::*;
pub use get_chan_info::*;
pub use input::*;
pub use input_mouse::*;
pub use option_value::*;
pub use set_client_info::*;
pub use set_var::*;
pub use ui_attach::*;
pub use ui_set_focus::*;
pub use ui_try_resize::*;
pub use ui_try_resize_grid::*;
pub use window::*;
