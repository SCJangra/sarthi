//! sarthi's RPC with Neovim: the connection and process (`client`), Neovim's API (`neovim`), and sarthi's own API
//! between sarthi and its Lua (`sarthi`, with the Lua in the workspace's `runtime/`).

pub mod client;
pub mod neovim;
pub mod sarthi;

// Within the crate, everything is at hand as `crate::Name`.
#[allow(unused_imports)]
use client::*;
#[allow(unused_imports)]
use neovim::*;
#[allow(unused_imports)]
use sarthi::*;

/// Derives decoding for enums tagged by their first element, like redraw events and requests (`[method, args…]`)
pub use sarthi_derive::DeserializeTaggedEnum;
