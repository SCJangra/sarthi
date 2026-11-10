use std::time::{Duration, Instant};
use std::{process, thread};

use flume::Receiver;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{
	Error, NvimClientInfo, NvimExecLua, NvimGetChanInfo, NvimInput, NvimInputMouse, NvimInputMouseParams, NvimMessage,
	NvimSetClientInfo, NvimSetVar, NvimUiAttach, NvimUiAttachParams, NvimUiSetFocus, NvimUiTryResize,
	NvimUiTryResizeParams, RpcClient,
};

/// How long to wait for Neovim to exit on its own before killing it. Generous, since killing it
/// while it writes ShaDa leaves a temporary file behind.
const EXIT_TIMEOUT: Duration = Duration::from_secs(3);

pub struct Nvim {
	// Fields drop in order: dropping `rpc` closes Neovim's input, which makes it exit, and then
	// `process` waits for that.
	rpc: RpcClient,
	#[allow(unused)]
	process: NvimProcess,
}

impl Nvim {
	/// Starts Neovim, embedded. Returns it and what it sends unasked (notifications and requests), which ends once
	/// Neovim exits.
	pub fn start() -> Result<(Self, Receiver<NvimMessage>), Error> {
		Self::spawn(&["--embed"])
	}

	/// Starts Neovim with `args`, which must include `--embed`
	fn spawn(args: &[&str]) -> Result<(Self, Receiver<NvimMessage>), Error> {
		let mut nvim = process::Command::new("nvim")
			.args(args)
			.stdin(process::Stdio::piped())
			.stdout(process::Stdio::piped())
			.spawn()
			.map_err(Error::SpawnNvim)?;

		let stdin = nvim.stdin.take().expect("stdin handle present");
		let stdout = nvim.stdout.take().expect("stdout handle present");

		let (rpc, messages) = RpcClient::start(stdout, stdin);

		Ok((Self { rpc, process: NvimProcess(nvim) }, messages))
	}

	/// The id of the channel nvui and Neovim talk on, e.g. for Lua code that calls back with `rpcrequest()`
	pub fn channel(&self) -> Result<u64, Error> {
		Ok(self.rpc.call::<NvimGetChanInfo>([0]).map_err(Error::Rpc)?.id)
	}

	/// Sets the global variable `g:name` (`vim.g.name` in Lua) to `value`, without waiting for Neovim
	pub fn set_var<V: Serialize>(&self, name: &str, value: V) -> Result<(), Error> {
		self.rpc.notify::<NvimSetVar<V>>((name.to_owned(), value)).map_err(Error::Rpc)
	}

	/// Identifies the client to Neovim, e.g. for Lua code looking for its channel
	pub fn set_client_info(&self, info: NvimClientInfo) -> Result<(), Error> {
		self.rpc.notify::<NvimSetClientInfo>(info).map_err(Error::Rpc)
	}

	/// Shows `message` as an error in Neovim (`vim.notify()` at ERROR level), without waiting for it
	pub fn show_error(&self, message: &str) -> Result<(), Error> {
		let code = String::from("vim.notify(..., vim.log.levels.ERROR)");
		self.rpc.notify::<NvimExecLua>((code, vec![message.into()])).map_err(Error::Rpc)
	}

	/// Runs Lua `code` in Neovim with `args`, a tuple (`()` for none) the code gets as `...`, and waits for its result
	pub fn exec_lua<R: DeserializeOwned>(&self, code: &str, args: impl Serialize) -> Result<R, Error> {
		let args = match rmpv::ext::to_value(args).map_err(|error| Error::Rpc(error.into()))? {
			rmpv::Value::Array(args) => args,
			rmpv::Value::Nil => Vec::new(),
			arg => vec![arg],
		};
		let result = self.rpc.call::<NvimExecLua>((code.to_owned(), args)).map_err(Error::Rpc)?;
		rmpv::ext::from_value(result).map_err(|error| Error::Rpc(error.into()))
	}

	/// Attaches as a UI and waits until Neovim accepted it
	pub fn ui_attach(&self, params: NvimUiAttachParams) -> Result<rmpv::Value, Error> {
		self.rpc.call::<NvimUiAttach>(params).map_err(Error::Rpc)
	}

	/// Asks Neovim to resize the UI, without waiting for it: the new size arrives with the redraws
	pub fn ui_try_resize(&self, params: NvimUiTryResizeParams) -> Result<(), Error> {
		self.rpc.notify::<NvimUiTryResize>(params).map_err(Error::Rpc)
	}

	/// Types `keys`, in `:help key-notation`, without waiting for Neovim to process them
	pub fn input(&self, keys: &str) -> Result<(), Error> {
		self.rpc.notify::<NvimInput>([keys.to_owned()]).map_err(Error::Rpc)
	}

	/// Sends a mouse event, without waiting for Neovim to process it
	pub fn input_mouse(&self, params: NvimInputMouseParams) -> Result<(), Error> {
		self.rpc.notify::<NvimInputMouse>(params).map_err(Error::Rpc)
	}

	/// Tells Neovim whether the UI has focus, for `FocusGained` and `FocusLost` autocommands
	pub fn ui_set_focus(&self, focused: bool) -> Result<(), Error> {
		self.rpc.notify::<NvimUiSetFocus>([focused]).map_err(Error::Rpc)
	}
}

/// The Neovim child process. When dropped, waits for it to exit, killing it if it doesn't exit within
/// [`EXIT_TIMEOUT`].
struct NvimProcess(process::Child);

impl Drop for NvimProcess {
	fn drop(&mut self) {
		let deadline = Instant::now() + EXIT_TIMEOUT;

		while Instant::now() < deadline {
			match self.0.try_wait() {
				Ok(Some(_)) | Err(_) => return,
				Ok(None) => thread::sleep(Duration::from_millis(10)),
			}
		}

		tracing::warn!("Neovim didn't exit within {EXIT_TIMEOUT:?}, killing it");
		let _ = self.0.kill();
		let _ = self.0.wait();
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use std::time::Instant;

	use super::{EXIT_TIMEOUT, Nvim};

	#[test]
	fn drop_waits_for_neovim_to_exit() {
		// Isolated from the user's config and ShaDa.
		let (nvim, _messages) = Nvim::spawn(&["--embed", "--clean", "-i", "NONE", "-n"]).unwrap();
		let pid = nvim.process.0.id();

		let start = Instant::now();
		drop(nvim);

		// Exiting on its own (not killed after the timeout), and reaped: no zombie left in /proc.
		assert!(start.elapsed() < EXIT_TIMEOUT, "Neovim had to be killed");
		assert!(!Path::new(&format!("/proc/{pid}")).exists(), "Neovim process {pid} still exists");
	}

	#[test]
	fn show_error() {
		let (nvim, _messages) = Nvim::spawn(&["--embed", "--clean", "-i", "NONE", "-n"]).unwrap();
		nvim.show_error("guifont: font not found: X").unwrap();
		// Neovim handles the notification first, then this call.
		let error: String = nvim.exec_lua("return vim.v.errmsg", ()).unwrap();
		assert_eq!(error, "guifont: font not found: X");

		// With an error shown, Neovim doesn't exit right away when its input closes; quitting spares the test the wait.
		nvim.exec_lua::<()>("vim.schedule(function() vim.cmd('qa!') end)", ()).unwrap();
	}

	#[test]
	fn channel_and_set_var() {
		let (nvim, _messages) = Nvim::spawn(&["--embed", "--clean", "-i", "NONE", "-n"]).unwrap();
		// The channel Neovim says we're on is the one Lua sees for us.
		let channel = nvim.channel().unwrap();
		let listed: u64 = nvim
			.exec_lua(
				"for _, chan in ipairs(vim.api.nvim_list_chans()) do if chan.mode == 'rpc' then return chan.id end end",
				(),
			)
			.unwrap();
		assert_eq!(channel, listed);

		nvim.set_var("nvui_test", (true, 3)).unwrap();
		let value: (bool, u32) = nvim.exec_lua("return vim.g.nvui_test", ()).unwrap();
		assert_eq!(value, (true, 3));
	}
}
