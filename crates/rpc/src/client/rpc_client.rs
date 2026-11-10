use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::{io, thread};

use flume::{Receiver, Sender, WeakSender};

use super::frame::FrameReader;
use crate::{
	IncomingRequest, IncomingRpcMessage, Message, OutgoingRpcMessage, Request, Responder, RpcError, RpcMethod,
	RpcRequest, RpcResponse,
};

/// A general msgpack RPC client
pub struct RpcClient {
	inner: Arc<RpcClientInner>,
	request: Sender<Vec<u8>>,

	/// Handle for the thread that reads messages from the server
	#[allow(unused)]
	read_handle: thread::JoinHandle<()>,

	/// Handle for the thread that writes messages to the server
	#[allow(unused)]
	write_handle: thread::JoinHandle<()>,
}

struct RpcClientInner {
	/// Id of next RPC request
	next_req_id: AtomicU32,

	/// Whether the RPC client is running or not
	running: AtomicBool,

	/// Calls waiting for their response, by request id. Only the calling thread and the reader touch it, briefly,
	/// so a plain mutex is enough.
	pending: Mutex<HashMap<u32, Sender<RpcResponse<rmpv::Value>>>>,

	/// Sends replies to the server's requests through the writer thread. Weak, so that it doesn't
	/// keep the writer running once [`RpcClient`] is stopped.
	replies: WeakSender<Vec<u8>>,
}

impl RpcClientInner {
	fn start<R, W>(
		self: Arc<Self>,
		reader: R,
		writer: W,
		request: Receiver<Vec<u8>>,
		messages: Sender<Message>,
	) -> (thread::JoinHandle<()>, thread::JoinHandle<()>)
	where
		R: io::Read + Send + Sync + 'static,
		W: io::Write + Send + Sync + 'static,
	{
		self.running.store(true, Ordering::SeqCst);

		let self_clone = self.clone();
		let write_handle = thread::spawn(move || {
			let mut writer = io::BufWriter::new(writer);

			// Runs until every sender is gone (the client was stopped or dropped) or the server can't
			// be written to anymore. Exiting drops the writer, which closes the server's input.
			while self_clone.running.load(Ordering::SeqCst) {
				let Ok(request) = request.recv() else { break };

				if let Err(err) = Self::send_request(request, &mut writer) {
					tracing::error!("Failed to send request: {err}");
					break;
				}
			}
		});

		// Exiting drops `messages`, which ends the stream of messages.
		let read_handle = thread::spawn(move || {
			let mut frames = FrameReader::new(reader);

			while self.running.load(Ordering::SeqCst) {
				let frame = match frames.next_frame() {
					Ok(Some(frame)) => frame,
					// E.g. Neovim exited after `:qa`
					Ok(None) => {
						tracing::debug!("RPC stream closed");
						break;
					},
					Err(err) => {
						tracing::error!("RPC stream closed: {err}");
						break;
					},
				};

				// Each message is framed before it is parsed, so one that doesn't match our types
				// is skipped without losing our place in the stream.
				let msg = match rmp_serde::from_slice::<IncomingRpcMessage<rmpv::Value>>(frame) {
					Ok(msg) => msg,
					Err(err) => {
						self.reject(frame, &err);
						continue;
					},
				};

				if let Err(err) = self.process_msg(msg, &messages) {
					tracing::error!("Failed to process message: {err}");
				}
			}

			self.disconnect();
		});

		(read_handle, write_handle)
	}

	/// Skips a message that `err` says doesn't match our types. If it's a request, e.g. with arguments of the wrong type,
	/// it's answered with the error, since Neovim waits for an answer.
	fn reject(&self, frame: &[u8], err: &rmp_serde::decode::Error) {
		let Ok((0, id, method, params)) = rmp_serde::from_slice::<(u8, u32, rmpv::Value, rmpv::Value)>(frame) else {
			tracing::error!("Skipping RPC message that failed to deserialize: {err}");
			return;
		};
		// Decoded again, through a value, for serde's message, e.g. "invalid type: string, expected u32", rather than
		// rmp_serde's about msgpack markers. Only mismatched requests take this way.
		let message = match rmpv::ext::from_value::<Request>(rmpv::Value::Array(vec![method, params])) {
			Err(rmpv::ext::Error::Syntax(message)) => message,
			Ok(_) => err.to_string(),
		};
		tracing::warn!("Request {id} from Neovim doesn't match: {message}");
		Responder::new(id, self.replies.clone()).respond::<()>(Err(message));
	}

	/// Marks the client as stopped once no more messages can arrive, and fails the calls waiting for a response.
	fn disconnect(&self) {
		self.running.store(false, Ordering::SeqCst);
		self.pending().clear();
	}

	fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Sender<RpcResponse<rmpv::Value>>>> {
		// The map stays consistent even if a thread panicked while holding the lock.
		self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
	}

	fn send_request<W>(request: Vec<u8>, writer: &mut W) -> Result<(), RpcError>
	where
		W: io::Write,
	{
		writer.write_all(&request).map_err(RpcError::FlushRequest)?;
		writer.flush().map_err(RpcError::FlushRequest)?;

		Ok(())
	}

	fn process_msg(&self, msg: IncomingRpcMessage<rmpv::Value>, messages: &Sender<Message>) -> Result<(), RpcError> {
		match msg {
			// If nobody receives it, the request is dropped, which answers it with an error.
			IncomingRpcMessage::Request { id, request } => {
				let request = IncomingRequest { request, responder: Responder::new(id, self.replies.clone()) };
				messages
					.send(Message::Request(request))
					.map_err(|_| RpcError::SendNotification)?;
			},
			IncomingRpcMessage::Notification(notification) => {
				messages
					.send(Message::Notification(notification))
					.map_err(|_| RpcError::SendNotification)?;
			},
			IncomingRpcMessage::Response(response) => {
				let Some(req) = self.pending().remove(&response.id) else { return Ok(()) };
				req.send(response).map_err(|_| RpcError::SendResponse)?;
			},
		}
		Ok(())
	}
}

impl RpcClient {
	/// Starts reading messages from `reader` and writing them to `writer`, each on its own thread. Returns the client
	/// and what the server sends unasked (notifications and requests), which ends once the connection closes.
	pub fn start<R, W>(reader: R, writer: W) -> (Self, Receiver<Message>)
	where
		R: io::Read + Send + Sync + 'static,
		W: io::Write + Send + Sync + 'static,
	{
		let (sender, receiver) = flume::unbounded();
		let (send_message, messages) = flume::unbounded();

		let inner = Arc::new(RpcClientInner {
			next_req_id: AtomicU32::new(0),
			running: AtomicBool::new(false),
			pending: Mutex::new(HashMap::new()),
			replies: sender.downgrade(),
		});

		let (read_handle, write_handle) = inner.clone().start(reader, writer, receiver, send_message);

		(Self { inner, request: sender, read_handle, write_handle }, messages)
	}

	/// Calls `M` and waits for Neovim's reply. Never call it from the reader thread, which delivers the reply.
	pub fn call<M>(&self, params: M::Params) -> Result<M::Response, RpcError>
	where
		M: RpcMethod,
	{
		let id = self.inner.next_req_id.fetch_add(1, Ordering::SeqCst);
		let (sender, receiver) = flume::bounded(1);
		self.inner.pending().insert(id, sender);

		// If the connection already closed, nothing will answer or clear this call.
		if !self.inner.running.load(Ordering::SeqCst) {
			self.inner.pending().remove(&id);
			return Err(RpcError::RequestDropped);
		}

		let req = OutgoingRpcMessage::Request(RpcRequest { id, method: M::METHOD.to_string(), params });
		let sent = rmp_serde::to_vec_named(&req)
			.map_err(RpcError::from)
			.and_then(|req| self.request.send(req).map_err(|_| RpcError::SendRequest));
		if let Err(err) = sent {
			self.inner.pending().remove(&id);
			return Err(err);
		}

		// The reader removes the call when its response arrives. The sender is dropped without a response when the
		// connection closes.
		let response = receiver.recv().map_err(|_| RpcError::RequestDropped)?;
		let response = response.result.map_err(RpcError::Response)?;

		let response: M::Response = rmpv::ext::from_value(response)?;
		Ok(response)
	}

	/// Calls `M` without waiting for a reply. Neovim processes it in order with other messages and reports errors
	/// from it with `nvim_error_event`.
	pub fn notify<M>(&self, params: M::Params) -> Result<(), RpcError>
	where
		M: RpcMethod,
	{
		let notification = OutgoingRpcMessage::Notification { method: M::METHOD, params };
		let notification = rmp_serde::to_vec_named(&notification)?;
		self.request.send(notification).map_err(|_| RpcError::SendRequest)
	}
}

#[cfg(test)]
mod tests {
	use std::io::{self, Read, Write};
	use std::sync::Arc;
	use std::time::Duration;

	use crate::{
		Eval, GridCursorGotoEvent, IncomingRequest, Input, Message, Notification, RedrawNotification, Register,
		Request, RpcClient, RpcError, UiAttach, UiAttachParams, UiOptions,
	};

	/// Runs `call` on another thread, so a call that never returns fails the test instead of hanging it
	fn within_5s<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> Option<T> {
		let (done, result) = flume::bounded(1);
		std::thread::spawn(move || done.send(call()));
		result.recv_timeout(Duration::from_secs(5)).ok()
	}

	/// Starts an embedded Neovim that doesn't touch the user's config, ShaDa or swap files.
	fn spawn_nvim() -> std::process::Child {
		std::process::Command::new("nvim")
			.args(["--embed", "--clean", "-i", "NONE", "-n"])
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::piped())
			.spawn()
			.unwrap()
	}

	#[test]
	fn call() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let (client, _messages) = RpcClient::start(stdout, stdin);

		let response = client.call::<Eval>(["1 + 1".into()]).unwrap();

		assert_eq!(response, rmpv::Value::from(2));

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn notify() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();
		let (client, _messages) = RpcClient::start(stdout, stdin);

		// With --embed, Neovim finishes starting up, and only then handles typed keys, once a UI attaches.
		let attach = UiAttachParams { width: 20, height: 5, options: UiOptions::all() };
		client.call::<UiAttach>(attach).unwrap();

		client.notify::<Input>(["ihello<lt>x<Esc>".into()]).unwrap();

		// nvim_input only queues the keys, and Neovim types them when its event loop gets to it.
		let typed = rmpv::Value::from("hello<x");
		let deadline = std::time::Instant::now() + Duration::from_secs(5);
		let mut line = rmpv::Value::Nil;
		while line != typed && std::time::Instant::now() < deadline {
			line = client.call::<Eval>(["getline(1)".into()]).unwrap();
		}

		assert_eq!(line, typed);

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn answer_unhandled_request_with_an_error() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let client = Arc::new(RpcClient::start(stdout, stdin).0);

		// While evaluating this, Neovim sends us a request on our channel (1) and blocks until we reply. Nobody
		// receives it here, so it's dropped, which answers it with an error.
		let caller = client.clone();
		let response = within_5s(move || caller.call::<Eval>(["rpcrequest(1, 'unknown')".into()]))
			.expect("Neovim is blocked on our reply");

		let err = response.unwrap_err();
		assert!(err.to_string().contains("not handled"), "unexpected error: {err}");

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn answer_request() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();
		let (client, messages) = RpcClient::start(stdout, stdin);

		// Neovim blocks in rpcrequest() until we answer, and our answer is what it evaluates to.
		let client = Arc::new(client);
		let caller = client.clone();
		let call = std::thread::spawn(move || caller.call::<Eval>(["rpcrequest(1, 'get_clipboard', '*')".into()]));

		let message = messages.recv_timeout(Duration::from_secs(5));
		let Ok(Message::Request(IncomingRequest { request: Request::GetClipboard(args), responder })) = message else {
			panic!("expected a get_clipboard request: {message:?}")
		};
		assert_eq!(args.register, Register::Primary);
		responder.respond(Ok("pasted"));

		assert_eq!(call.join().unwrap().unwrap(), rmpv::Value::from("pasted"));

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn answer_mismatched_arguments_with_the_error() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();
		let (client, messages) = RpcClient::start(stdout, stdin);

		// The register is a number: the request can't be decoded, so the client answers it, and the app never sees it.
		let client = Arc::new(client);
		let caller = client.clone();
		let response = within_5s(move || caller.call::<Eval>(["rpcrequest(1, 'get_clipboard', 3)".into()]))
			.expect("Neovim is blocked on our reply");

		let err = response.unwrap_err();
		let err = err.to_string();
		assert!(err.contains("invalid type: integer `3`, expected a string"), "unexpected error: {err}");
		assert!(messages.try_recv().is_err());

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn call_nvim_ui_attach() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let (client, _messages) = RpcClient::start(stdout, stdin);

		let params = UiAttachParams { width: 10, height: 10, options: UiOptions::all() };
		let response = client.call::<UiAttach>(params).unwrap();

		assert_eq!(response, rmpv::Value::Nil);

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	/// Writer that answers the first request by writing `reply` into the client's input pipe.
	struct Responder {
		pipe: io::PipeWriter,
		reply: Option<Vec<u8>>,
	}

	impl Write for Responder {
		fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
			if let Some(reply) = self.reply.take() {
				self.pipe.write_all(&reply)?;
			}
			Ok(buf.len())
		}

		fn flush(&mut self) -> io::Result<()> {
			Ok(())
		}
	}

	/// A message that doesn't match our types is skipped on its own: the messages after it are still
	/// delivered and nothing else is reported in its place.
	#[test]
	fn deliver_messages_after_one_that_fails_to_deserialize() {
		// [2, "redraw", [["grid_line", ["x", 0, 0, [["a"]], false]]]]: the grid id must be an integer
		let bad_notification = [
			&[0x93, 0x02, 0xA6][..],
			b"redraw",
			&[0x91, 0x92, 0xA9],
			b"grid_line",
			&[0x95, 0xA1, b'x', 0x00, 0x00, 0x91, 0x91, 0xA1, b'a', 0xC2],
		]
		.concat();

		// [2, "redraw", [["grid_cursor_goto", [1, 2, 3]]]]
		let notification =
			[&[0x93, 0x02, 0xA6][..], b"redraw", &[0x91, 0x92, 0xB0], b"grid_cursor_goto", &[0x93, 0x01, 0x02, 0x03]]
				.concat();

		// [1, 0, nil, 2]: response to the first request
		let response = [0x94, 0x01, 0x00, 0xC0, 0x02];

		let (reader, pipe) = io::pipe().unwrap();
		let reply = [&bad_notification[..], &notification, &response].concat();
		let (client, messages) = RpcClient::start(reader, Responder { pipe, reply: Some(reply) });

		let client = Arc::new(client);
		let caller = client.clone();
		let response =
			within_5s(move || caller.call::<Eval>(["1 + 1".into()])).expect("response after bad message was lost");

		assert_eq!(response.unwrap(), rmpv::Value::from(2));

		let expected = Notification::Redraw(vec![RedrawNotification::GridCursorGoto(vec![GridCursorGotoEvent {
			grid: 1,
			row: 2,
			col: 3,
		}])]);
		let Ok(Message::Notification(notification)) = messages.try_recv() else { panic!("expected a notification") };
		assert_eq!(notification, expected);
		assert!(messages.try_recv().is_err());
	}

	#[test]
	fn writer_exits_when_client_is_dropped() {
		// Keep the client's input open, so only the writer side can end.
		let (reader, _input) = io::pipe().unwrap();
		let (mut output, writer) = io::pipe().unwrap();

		drop(RpcClient::start(reader, writer));

		// The writer thread drops its end of the pipe when it exits, which ends this read.
		let (done, finished) = flume::bounded(1);
		std::thread::spawn(move || {
			let _ = output.read_to_end(&mut Vec::new());
			let _ = done.send(());
		});

		assert!(finished.recv_timeout(Duration::from_secs(5)).is_ok(), "writer thread is still running");
	}

	/// Writer that closes the client's input pipe as soon as the first request is written.
	struct CloseOnWrite(Option<io::PipeWriter>);

	impl Write for CloseOnWrite {
		fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
			self.0.take();
			Ok(buf.len())
		}

		fn flush(&mut self) -> io::Result<()> {
			Ok(())
		}
	}

	#[test]
	fn fail_pending_call_when_connection_closes() {
		let (reader, pipe) = io::pipe().unwrap();
		let (client, messages) = RpcClient::start(reader, CloseOnWrite(Some(pipe)));

		let response =
			within_5s(move || client.call::<Eval>(["1 + 1".into()])).expect("pending call hung after disconnect");

		assert!(matches!(response, Err(RpcError::RequestDropped)), "unexpected response: {response:?}");
		assert!(matches!(messages.recv_timeout(Duration::from_secs(5)), Err(flume::RecvTimeoutError::Disconnected)));
	}
}
