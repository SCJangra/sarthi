use flume::WeakSender;
use serde::Serialize;

use crate::{Notification, OutgoingRpcMessage, Request};

/// What Neovim sends without being asked, in the order it sent it: its notifications, and the requests of sarthi's Lua
#[derive(Debug)]
pub enum Message {
	Notification(Notification),
	Request(IncomingRequest),
}

/// A request from Neovim, e.g. from `rpcrequest()` on sarthi's channel, and what answers it
#[derive(Debug)]
pub struct IncomingRequest {
	pub request: Request,
	pub responder: Responder,
}

/// Answers a request. Neovim waits until it's answered, so a responder dropped without answering answers with an
/// error.
#[derive(Debug)]
pub struct Responder {
	id: u32,

	/// Sends the response through the client's writer, while the client still runs
	replies: WeakSender<Vec<u8>>,
	answered: bool,
}

impl Responder {
	pub(crate) fn new(id: u32, replies: WeakSender<Vec<u8>>) -> Self {
		Self { id, replies, answered: false }
	}

	/// Answers with `result`, or with an error carrying the message
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

impl Drop for Responder {
	fn drop(&mut self) {
		if !self.answered {
			tracing::warn!("Request {} from Neovim not answered", self.id);
			self.send(error_response(self.id, String::from("not handled")));
		}
	}
}

fn error_response(id: u32, message: String) -> Vec<u8> {
	rmp_serde::to_vec_named(&OutgoingRpcMessage::<()>::ErrorResponse { id, message })
		.expect("an id and a string always encode")
}
