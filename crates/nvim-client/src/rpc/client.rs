use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::{io, thread};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use flume::{Receiver, Sender, WeakSender};

use crate::rpc::frame::FrameReader;
use crate::{
	IncomingRpcMessage, NvimNotification, OutgoingRpcMessage, RpcError, RpcMethod, RpcRequest,
	RpcResponse,
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

	/// Pending ['RpcRequest']s
	pending: DashMap<u32, Sender<RpcResponse<rmpv::Value>>>,

	/// All the subscriptions to [`NvimNotification`]s
	subscription: ArcSwap<Option<Sender<NvimNotification>>>,

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
	) -> (thread::JoinHandle<()>, thread::JoinHandle<()>)
	where
		R: io::Read + Send + Sync + 'static,
		W: io::Write + Send + Sync + 'static,
	{
		self.running.store(true, Ordering::SeqCst);

		let self_clone = self.clone();
		let write_handle = thread::spawn(move || {
			let mut writer = io::BufWriter::new(writer);

			loop {
				if !self_clone.running.load(Ordering::SeqCst) {
					break;
				}

				#[rustfmt::skip]
				let Ok(request) = request
					.recv()
					.inspect_err(|err| tracing::error!("Receiving request on channel failed: {err}"))
					 else { continue };

				#[rustfmt::skip]
				let Err(err) = Self::send_request(request, &mut writer) else { continue; };
				tracing::error!("Failed to send request: {err}");
			}
		});

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

				if let Err(err) = self.process_msg(msg) {
					tracing::error!("Failed to process message: {err}");
				}
			}
		});

		(read_handle, write_handle)
	}

	fn send_request<W>(request: Vec<u8>, writer: &mut W) -> Result<(), RpcError>
	where
		W: io::Write,
	{
		writer.write_all(&request).map_err(RpcError::FlushRequest)?;
		writer.flush().map_err(RpcError::FlushRequest)?;

		Ok(())
	}

	fn process_msg(&self, msg: IncomingRpcMessage<rmpv::Value>) -> Result<(), RpcError> {
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
				let sub = self.subscription.load();

				#[rustfmt::skip]
				let Some(sub) = sub.as_ref() else { return Ok(()); };
				sub.send(notification).map_err(|_| RpcError::SendNotification)?;
			},
			IncomingRpcMessage::Response(response) => {
				#[rustfmt::skip]
				let Some(req) = self.pending.get(&response.id) else { return Ok(()) };
				req.send(response).map_err(|_| RpcError::SendResponse)?;
			},
		}
		Ok(())
	}
}

impl RpcClient {
	pub fn start<R, W>(reader: R, writer: W) -> Self
	where
		R: io::Read + Send + Sync + 'static,
		W: io::Write + Send + Sync + 'static,
	{
		let (sender, receiver) = flume::unbounded();

		let inner = Arc::new(RpcClientInner {
			next_req_id: AtomicU32::new(0),
			running: AtomicBool::new(false),
			pending: DashMap::new(),
			subscription: ArcSwap::new(Arc::new(None)),
			replies: sender.downgrade(),
		});

		let (read_handle, write_handle) = inner.clone().start(reader, writer, receiver);

		Self { inner, request: sender, read_handle, write_handle }
	}

	pub async fn call_method<M>(&self, id: u32, params: M::Params) -> Result<M::Response, RpcError>
	where
		M: RpcMethod,
	{
		let (sender, receiver) = flume::bounded(1);

		self.inner.pending.insert(id, sender);

		let req =
			OutgoingRpcMessage::Request(RpcRequest { id, method: M::METHOD.to_string(), params });
		let req = rmp_serde::to_vec_named(&req)?;

		self.request.send(req).map_err(|_| RpcError::SendRequest)?;

		let response = receiver.recv_async().await?.result.map_err(RpcError::Response)?;

		let response: M::Response = rmpv::ext::from_value(response)?;
		Ok(response)
	}

	pub async fn call<M>(&self, params: M::Params) -> Result<M::Response, RpcError>
	where
		M: RpcMethod,
	{
		let id = self.inner.next_req_id.fetch_add(1, Ordering::SeqCst);
		let res = self.call_method::<M>(id, params).await;
		self.inner.pending.remove(&id);
		res
	}

	pub fn subscribe(&self) -> Result<Receiver<NvimNotification>, RpcError> {
		if self.inner.subscription.load().is_some() {
			return Err(RpcError::AlreadySubscribed);
		}
		let (sender, receiver) = flume::unbounded();

		self.inner.subscription.store(Arc::new(Some(sender)));

		Ok(receiver)
	}

	pub fn unsubscribe(&self) {
		self.inner.subscription.store(Arc::new(None));
	}

	pub fn stop(self) {
		let Self { inner, request, read_handle, write_handle } = self;

		inner.running.store(false, Ordering::SeqCst);
		drop(request);
		let _ = read_handle.join();
		let _ = write_handle.join();
	}
}

#[cfg(test)]
mod tests {
	use std::io::{self, Write};
	use std::time::Duration;

	use crate::{
		GridCursorGotoEvent, NvimEval, NvimNotification, NvimUiAttach, NvimUiAttachParams, NvimUiOptions,
		RedrawNotification, RpcClient,
	};

	#[test]
	fn call() {
		let mut nvim = std::process::Command::new("nvim")
			.arg("--embed")
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::piped())
			.spawn()
			.unwrap();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let client = RpcClient::start(stdout, stdin);

		let future = client.call::<NvimEval>(["1 + 1".into()]);
		let response = smol::block_on(future).unwrap();

		assert_eq!(response, rmpv::Value::from(2));

		client.stop();
		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn reply_error_to_unsupported_request() {
		let mut nvim = std::process::Command::new("nvim")
			.arg("--embed")
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::piped())
			.spawn()
			.unwrap();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let client = RpcClient::start(stdout, stdin);

		// While evaluating this, Neovim sends us a request on our channel (1) and blocks until we reply.
		let call = async { Some(client.call::<NvimEval>(["rpcrequest(1, 'nvui_test')".into()]).await) };
		let timeout = async {
			smol::Timer::after(Duration::from_secs(5)).await;
			None
		};
		let response = smol::block_on(smol::future::or(call, timeout)).expect("Neovim is blocked on our reply");

		let err = response.unwrap_err();
		assert!(err.to_string().contains("Unknown method: nvui_test"), "unexpected error: {err}");

		client.stop();
		nvim.kill().unwrap();
		nvim.wait().unwrap();
	}

	#[test]
	fn call_nvim_ui_attach() {
		let mut nvim = std::process::Command::new("nvim")
			.arg("--embed")
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::piped())
			.spawn()
			.unwrap();

		let stdin = nvim.stdin.take().unwrap();
		let stdout = nvim.stdout.take().unwrap();

		let client = RpcClient::start(stdout, stdin);

		let future = client.call::<NvimUiAttach>(NvimUiAttachParams {
			width: 10,
			height: 10,
			options: NvimUiOptions::all(),
		});
		let response = smol::block_on(future).unwrap();

		assert_eq!(response, rmpv::Value::Nil);

		client.stop();
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
		let client = RpcClient::start(reader, Responder { pipe, reply: Some(reply) });
		let notifications = client.subscribe().unwrap();

		let call = async { Some(client.call::<NvimEval>(["1 + 1".into()]).await) };
		let timeout = async {
			smol::Timer::after(Duration::from_secs(5)).await;
			None
		};
		let response = smol::block_on(smol::future::or(call, timeout)).expect("response after bad message was lost");

		assert_eq!(response.unwrap(), rmpv::Value::from(2));

		let expected = NvimNotification::Redraw(vec![RedrawNotification::GridCursorGoto(vec![GridCursorGotoEvent {
			grid: 1,
			row: 2,
			col: 3,
		}])]);
		assert_eq!(notifications.try_recv().unwrap(), expected);
		assert!(notifications.try_recv().is_err());

		client.stop();
	}
}
