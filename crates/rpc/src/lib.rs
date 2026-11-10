//! nvui's RPC with Neovim: the connection and process (`client`), Neovim's API (`neovim`), and nvui's own API between
//! nvui and its Lua (`nvui`, with the Lua in the workspace's `runtime/`).

pub mod client;
pub mod neovim;
pub mod nvui;

// Within the crate, everything is at hand as `crate::Name`.
#[allow(unused_imports)]
use client::*;
#[allow(unused_imports)]
use neovim::*;
#[allow(unused_imports)]
use nvui::*;

/// Derives decoding for enums tagged by their first element, like redraw events and requests (`[method, args…]`)
pub use nvui_derive::DeserializeTaggedEnum;
