use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::{io, thread};

use flume::{Receiver, Sender, WeakSender};

use crate::rpc::frame::FrameReader;
use crate::{IncomingRpcMessage, NvimNotification, OutgoingRpcMessage, RpcError, RpcMethod, RpcRequest, RpcResponse};

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
		notifications: Sender<NvimNotification>,
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

		// Exiting drops `notifications`, which ends the stream of notifications.
		let read_handle = thread::spawn(move || {
			let mut frames = FrameReader::new(reader);

			while self.running.load(Ordering::SeqCst) {
				let frame = match frames.next_frame() {
					Ok(frame) => frame,
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
						tracing::error!("Skipping RPC message that failed to deserialize: {err}");
						continue;
					},
				};

				if let Err(err) = self.process_msg(msg, &notifications) {
					tracing::error!("Failed to process message: {err}");
				}
			}

			self.disconnect();
		});

		(read_handle, write_handle)
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

	fn process_msg(
		&self,
		msg: IncomingRpcMessage<rmpv::Value>,
		notifications: &Sender<NvimNotification>,
	) -> Result<(), RpcError> {
		match msg {
			IncomingRpcMessage::Request(req) => {
				// No requests are handled yet. Reply with an error, so the caller's `rpcrequest()`
				// fails instead of blocking Neovim while it waits for a response.
				tracing::warn!("Received unsupported RPC request: {}", req.method);

				let reply = OutgoingRpcMessage::<()>::ErrorResponse {
					id: req.id,
					message: format!("Unknown method: {}", req.method),
				};
				let reply = rmp_serde::to_vec_named(&reply)?;

				let Some(replies) = self.replies.upgrade() else { return Ok(()) };
				replies.send(reply).map_err(|_| RpcError::SendRequest)?;
			},
			IncomingRpcMessage::Notification(notification) => {
				notifications.send(notification).map_err(|_| RpcError::SendNotification)?;
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
	/// and the server's notifications, which end once the connection closes.
	pub fn start<R, W>(reader: R, writer: W) -> (Self, Receiver<NvimNotification>)
	where
		R: io::Read + Send + Sync + 'static,
		W: io::Write + Send + Sync + 'static,
	{
		let (sender, receiver) = flume::unbounded();
		let (notify, notifications) = flume::unbounded();

		let inner = Arc::new(RpcClientInner {
			next_req_id: AtomicU32::new(0),
			running: AtomicBool::new(false),
			pending: Mutex::new(HashMap::new()),
			replies: sender.downgrade(),
		});

		let (read_handle, write_handle) = inner.clone().start(reader, writer, receiver, notify);

		(Self { inner, request: sender, read_handle, write_handle }, notifications)
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
		GridCursorGotoEvent, NvimEval, NvimInput, NvimNotification, NvimUiAttach, NvimUiAttachParams, NvimUiOptions,
		RedrawNotification, RpcClient, RpcError,
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

		let (client, _notifications) = RpcClient::start(stdout, stdin);

		let response = client.call::<NvimEval>(["1 + 1".into()]).unwrap();

		assert_eq!(response, rmpv::Value::from(2));

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn notify() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();
		let (client, _notifications) = RpcClient::start(stdout, stdin);

		// With --embed, Neovim finishes starting up, and only then handles typed keys, once a UI attaches.
		let attach = NvimUiAttachParams { width: 20, height: 5, options: NvimUiOptions::all() };
		client.call::<NvimUiAttach>(attach).unwrap();

		client.notify::<NvimInput>(["ihello<lt>x<Esc>".into()]).unwrap();

		// nvim_input only queues the keys, and Neovim types them when its event loop gets to it.
		let typed = rmpv::Value::from("hello<x");
		let deadline = std::time::Instant::now() + Duration::from_secs(5);
		let mut line = rmpv::Value::Nil;
		while line != typed && std::time::Instant::now() < deadline {
			line = client.call::<NvimEval>(["getline(1)".into()]).unwrap();
		}

		assert_eq!(line, typed);

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn reply_error_to_unsupported_request() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let client = Arc::new(RpcClient::start(stdout, stdin).0);

		// While evaluating this, Neovim sends us a request on our channel (1) and blocks until we reply.
		let caller = client.clone();
		let response = within_5s(move || caller.call::<NvimEval>(["rpcrequest(1, 'nvui_test')".into()]))
			.expect("Neovim is blocked on our reply");

		let err = response.unwrap_err();
		assert!(err.to_string().contains("Unknown method: nvui_test"), "unexpected error: {err}");

		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn call_nvim_ui_attach() {
		let mut nvim = spawn_nvim();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let (client, _notifications) = RpcClient::start(stdout, stdin);

		let params = NvimUiAttachParams { width: 10, height: 10, options: NvimUiOptions::all() };
		let response = client.call::<NvimUiAttach>(params).unwrap();

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
		let (client, notifications) = RpcClient::start(reader, Responder { pipe, reply: Some(reply) });

		let client = Arc::new(client);
		let caller = client.clone();
		let response =
			within_5s(move || caller.call::<NvimEval>(["1 + 1".into()])).expect("response after bad message was lost");

		assert_eq!(response.unwrap(), rmpv::Value::from(2));

		let expected = NvimNotification::Redraw(vec![RedrawNotification::GridCursorGoto(vec![GridCursorGotoEvent {
			grid: 1,
			row: 2,
			col: 3,
		}])]);
		assert_eq!(notifications.try_recv().unwrap(), expected);
		assert!(notifications.try_recv().is_err());
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
		let (client, notifications) = RpcClient::start(reader, CloseOnWrite(Some(pipe)));

		let response =
			within_5s(move || client.call::<NvimEval>(["1 + 1".into()])).expect("pending call hung after disconnect");

		assert!(matches!(response, Err(RpcError::RequestDropped)), "unexpected response: {response:?}");
		assert!(matches!(
			notifications.recv_timeout(Duration::from_secs(5)),
			Err(flume::RecvTimeoutError::Disconnected)
		));
	}
}
