use std::io;

use crate::RpcError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("SpawnNvim({0})")]
	SpawnNvim(io::Error),

	#[error("Rpc({0})")]
	Rpc(RpcError),
}
