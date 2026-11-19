//! Draws Neovim's grids with wgpu.
//!
//! The editor turns Neovim's state into [`Layer`]s of cells, which [`Renderer`] draws: their text, backgrounds,
//! decorations and frames, the cursor, and the animations of scrolling and the cursor. It knows nothing about Neovim.

#![warn(missing_docs)]

mod animation;
mod builtin_glyphs;
mod color;
mod config;
mod cursor;
mod error;
mod font;
mod geometry;
mod layer;
mod quad;
mod renderer;
mod scroll;
mod shape;
mod stencil;
#[cfg(test)]
mod test_gpu;
mod text;

pub use animation::{Easing, Motion};
pub use config::*;
pub use cursor::CursorMotion;
pub use error::*;
pub use font::*;
pub use geometry::{Edges, Rect};
pub use layer::*;
pub use renderer::*;
