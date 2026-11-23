//! What can go wrong with a call or a notification.

use std::io;

/// What can go wrong with a call or a notification.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
	/// A value didn't decode as the type expected.
	#[error("Rmpv({0})")]
	Rmpv(#[from] rmpv::ext::Error),

	/// A message couldn't be encoded.
	#[error("Rmp({0})")]
	RmpWrite(#[from] rmp_serde::encode::Error),

	/// The writer thread is gone, so nothing more can be sent.
	#[error("SendRequest")]
	SendRequest,

	/// Neovim's input couldn't be written to.
	#[error("WriteRequest({0})")]
	FlushRequest(io::Error),

	/// The caller waiting for a response is gone.
	#[error("SendResponse")]
	SendResponse,

	/// Nobody receives Neovim's messages anymore.
	#[error("SendNotification")]
	SendNotification,

	/// Neovim raised an error for the call.
	#[error("Response({0})")]
	Response(rmpv::Value),

	/// The connection closed before Neovim answered.
	#[error("RequestDropped")]
	RequestDropped,
}
