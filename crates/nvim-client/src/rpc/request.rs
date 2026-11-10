use flume::WeakSender;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{NvimNotification, OutgoingRpcMessage};

/// What Neovim sends without being asked, in the order it sent it
#[derive(Debug)]
pub enum NvimMessage {
	Notification(NvimNotification),
	Request(NvimRequest),
}

/// A request from Neovim, e.g. a plugin calling `rpcrequest()` on nvui's channel. Neovim waits until it's answered,
/// so a request dropped without a response is answered with an error.
#[derive(Debug)]
pub struct NvimRequest {
	id: u32,
	method: String,
	params: rmpv::Value,

	/// Sends the response through the client's writer, while the client still runs
	replies: WeakSender<Vec<u8>>,
	answered: bool,
}

impl NvimRequest {
	pub(crate) fn new(id: u32, method: String, params: rmpv::Value, replies: WeakSender<Vec<u8>>) -> Self {
		Self { id, method, params, replies, answered: false }
	}

	pub fn method(&self) -> &str {
		&self.method
	}

	pub fn params<T: DeserializeOwned>(&self) -> Result<T, String> {
		rmpv::ext::from_value(self.params.clone()).map_err(|rmpv::ext::Error::Syntax(message)| message)
	}

	/// Answers the request: with `result`, or with an error carrying the message
	pub fn respond<T: Serialize>(mut self, result: Result<T, String>) {
		self.answered = true;
		let id = self.id;
		let response = result.and_then(|result| {
			rmp_serde::to_vec_named(&OutgoingRpcMessage::Response { id, result })
				.map_err(|err| format!("Couldn't encode the response: {err}"))
		});
		self.send(response.unwrap_or_else(|message| error_response(id, message)));
	}

	fn send(&self, response: Vec<u8>) {
		if let Some(replies) = self.replies.upgrade() {
			let _ = replies.send(response);
		}
	}
}

impl Drop for NvimRequest {
	fn drop(&mut self) {
		if !self.answered {
			tracing::warn!("Unhandled request from Neovim: {}", self.method);
			self.send(error_response(self.id, format!("Unknown method: {}", self.method)));
		}
	}
}

fn error_response(id: u32, message: String) -> Vec<u8> {
	rmp_serde::to_vec_named(&OutgoingRpcMessage::<()>::ErrorResponse { id, message })
		.expect("an id and a string always encode")
}
