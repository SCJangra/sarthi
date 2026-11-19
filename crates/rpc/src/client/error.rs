//! What can go wrong talking to Neovim, or starting it

use std::io;

use crate::RpcError;

/// What can go wrong starting Neovim or talking to it
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// Neovim couldn't be started
	#[error("SpawnNvim({0})")]
	SpawnNvim(io::Error),

	/// A call or notification failed
	#[error("Rpc({0})")]
	Rpc(RpcError),
}
