//! Neovim's API: the functions nvui calls (`methods`) and the notifications Neovim sends, mostly redraw events

mod methods;
mod notification;
mod redraw;

pub use methods::*;
pub use notification::*;
pub use redraw::*;
