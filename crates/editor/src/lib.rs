//! Neovim's UI state, from its redraw events, and the layers the renderer draws from it. Knows nothing about windows.

pub mod compositor;
pub mod cursor;
pub mod grid;
pub mod highlight;
