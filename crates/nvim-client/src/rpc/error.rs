use std::io;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
	#[error("Rmpv({0})")]
	Rmpv(#[from] rmpv::ext::Error),

	#[error("Rmp({0})")]
	RmpWrite(#[from] rmp_serde::encode::Error),

	#[error("SendRequest")]
	SendRequest,

	#[error("WriteRequest({0})")]
	FlushRequest(io::Error),

	#[error("SendResponse")]
	SendResponse,

	#[error("SendNotification")]
	SendNotification,

	#[error("Response({0})")]
	Response(rmpv::Value),

	#[error("RequestDropped")]
	RequestDropped,
}
