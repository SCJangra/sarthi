//! Neovim: its process and the API functions sarthi calls

use std::ffi::{OsStr, OsString};
use std::time::{Duration, Instant};
use std::{process, thread};

use flume::Receiver;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{
	ClientInfo, Error, ExecLua, GetChanInfo, Input, InputMouse, InputMouseParams, Message, RpcClient, SetClientInfo,
	SetVar, UiAttach, UiAttachParams, UiSetFocus, UiTryResize, UiTryResizeGrid, UiTryResizeGridParams,
	UiTryResizeParams,
};

/// How long to wait for Neovim to exit on its own before killing it. Generous, since killing it
/// while it writes ShaDa leaves a temporary file behind.
const EXIT_TIMEOUT: Duration = Duration::from_secs(3);

/// Neovim, running embedded, and the API functions sarthi calls
pub struct Nvim {
	// Fields drop in order: dropping `rpc` closes Neovim's input, which makes it exit, and then
	// `process` waits for that.
	rpc: RpcClient,
	#[allow(unused)]
	process: NvimProcess,
}

impl Nvim {
	/// Starts Neovim, `program`, embedded, with `args` too, e.g. files to edit, in environment `env`, or sarthi's for
	/// `None`. Returns it and what it sends unasked (notifications, and requests from sarthi's Lua), which ends once
	/// Neovim exits.
	pub fn start(
		program: &OsStr,
		args: &[impl AsRef<OsStr>],
		env: Option<&[(OsString, OsString)]>,
	) -> Result<(Self, Receiver<Message>), Error> {
		let args = [OsStr::new("--embed")].into_iter().chain(args.iter().map(AsRef::as_ref));
		Self::spawn(program, args, env)
	}

	/// Starts Neovim, `program`, with `args`, which must include `--embed`
	fn spawn<'a>(
		program: &OsStr,
		args: impl IntoIterator<Item = &'a OsStr>,
		env: Option<&[(OsString, OsString)]>,
	) -> Result<(Self, Receiver<Message>), Error> {
		let mut command = process::Command::new(program);
		// Only what's given, e.g. the login shell's
		if let Some(env) = env {
			command.env_clear().envs(env.iter().map(|(name, value)| (name, value)));
		}
		let mut nvim = command
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

	/// The id of the channel sarthi and Neovim talk on, e.g. for Lua code that calls back with `rpcrequest()`
	pub fn channel(&self) -> Result<u64, Error> {
		Ok(self.rpc.call::<GetChanInfo>([0]).map_err(Error::Rpc)?.id)
	}

	/// Sets the global variable `g:name` (`vim.g.name` in Lua) to `value`, without waiting for Neovim
	pub fn set_var<V: Serialize>(&self, name: &str, value: V) -> Result<(), Error> {
		self.rpc.notify::<SetVar<V>>((name.to_owned(), value)).map_err(Error::Rpc)
	}

	/// Identifies the client to Neovim, e.g. for Lua code looking for its channel
	pub fn set_client_info(&self, info: ClientInfo) -> Result<(), Error> {
		self.rpc.notify::<SetClientInfo>(info).map_err(Error::Rpc)
	}

	/// Shows `message` as an error in Neovim (`vim.notify()` at ERROR level), without waiting for it
	pub fn show_error(&self, message: &str) -> Result<(), Error> {
		self.show("ERROR", message)
	}

	/// Shows `message` as a warning in Neovim (`vim.notify()` at WARN level), without waiting for it
	pub fn show_warning(&self, message: &str) -> Result<(), Error> {
		self.show("WARN", message)
	}

	fn show(&self, level: &str, message: &str) -> Result<(), Error> {
		let code = format!("vim.notify(..., vim.log.levels.{level})");
		self.rpc.notify::<ExecLua>((code, vec![message.into()])).map_err(Error::Rpc)
	}

	/// Runs Lua `code` in Neovim with `args`, a tuple (`()` for none) the code gets as `...`, and waits for its result
	pub fn exec_lua<R: DeserializeOwned>(&self, code: &str, args: impl Serialize) -> Result<R, Error> {
		let args = match rmpv::ext::to_value(args).map_err(|error| Error::Rpc(error.into()))? {
			rmpv::Value::Array(args) => args,
			rmpv::Value::Nil => Vec::new(),
			arg => vec![arg],
		};
		let result = self.rpc.call::<ExecLua>((code.to_owned(), args)).map_err(Error::Rpc)?;
		rmpv::ext::from_value(result).map_err(|error| Error::Rpc(error.into()))
	}

	/// Attaches as a UI and waits until Neovim accepted it
	pub fn ui_attach(&self, params: UiAttachParams) -> Result<rmpv::Value, Error> {
		self.rpc.call::<UiAttach>(params).map_err(Error::Rpc)
	}

	/// Asks Neovim to resize the UI, without waiting for it: the new size arrives with the redraws
	pub fn ui_try_resize(&self, params: UiTryResizeParams) -> Result<(), Error> {
		self.rpc.notify::<UiTryResize>(params).map_err(Error::Rpc)
	}

	/// Asks Neovim to resize the window of a grid, without waiting for it: the new size arrives with the redraws
	pub fn ui_try_resize_grid(&self, params: UiTryResizeGridParams) -> Result<(), Error> {
		self.rpc.notify::<UiTryResizeGrid>(params).map_err(Error::Rpc)
	}

	/// Types `keys`, in `:help key-notation`, without waiting for Neovim to process them
	pub fn input(&self, keys: &str) -> Result<(), Error> {
		self.rpc.notify::<Input>([keys.to_owned()]).map_err(Error::Rpc)
	}

	/// Asks Neovim to quit as `:confirm qa` does, like gVim's close button: it asks about unsaved changes first, and
	/// may not quit. Typed as keys, so Neovim takes it in any mode, without waiting for it.
	pub fn ask_to_quit(&self) -> Result<(), Error> {
		self.input("<Cmd>confirm qa<CR>")
	}

	/// Sends a mouse event, without waiting for Neovim to process it
	pub fn input_mouse(&self, params: InputMouseParams) -> Result<(), Error> {
		self.rpc.notify::<InputMouse>(params).map_err(Error::Rpc)
	}

	/// Tells Neovim whether the UI has focus, for `FocusGained` and `FocusLost` autocommands
	pub fn ui_set_focus(&self, focused: bool) -> Result<(), Error> {
		self.rpc.notify::<UiSetFocus>([focused]).map_err(Error::Rpc)
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

	use std::ffi::OsStr;

	use super::{EXIT_TIMEOUT, Nvim};

	#[test]
	fn drop_waits_for_neovim_to_exit() {
		// Isolated from the user's config and ShaDa.
		let (nvim, _messages) = Nvim::start(OsStr::new("nvim"), &["--clean", "-i", "NONE", "-n"], None).unwrap();
		let pid = nvim.process.0.id();

		let start = Instant::now();
		drop(nvim);

		// Exiting on its own (not killed after the timeout), and reaped: no zombie left in /proc.
		assert!(start.elapsed() < EXIT_TIMEOUT, "Neovim had to be killed");
		assert!(!Path::new(&format!("/proc/{pid}")).exists(), "Neovim process {pid} still exists");
	}

	#[test]
	fn show_error() {
		let (nvim, _messages) = Nvim::start(OsStr::new("nvim"), &["--clean", "-i", "NONE", "-n"], None).unwrap();
		nvim.show_error("guifont: font not found: X").unwrap();
		// Neovim handles the notification first, then this call.
		let error: String = nvim.exec_lua("return vim.v.errmsg", ()).unwrap();
		assert_eq!(error, "guifont: font not found: X");

		// With an error shown, Neovim doesn't exit right away when its input closes; quitting spares the test the wait.
		nvim.exec_lua::<()>("vim.schedule(function() vim.cmd('qa!') end)", ()).unwrap();
	}

	#[test]
	fn channel_and_set_var() {
		let (nvim, _messages) = Nvim::start(OsStr::new("nvim"), &["--clean", "-i", "NONE", "-n"], None).unwrap();
		// The channel Neovim says we're on is the one Lua sees for us.
		let channel = nvim.channel().unwrap();
		let listed: u64 = nvim
			.exec_lua(
				"for _, chan in ipairs(vim.api.nvim_list_chans()) do if chan.mode == 'rpc' then return chan.id end end",
				(),
			)
			.unwrap();
		assert_eq!(channel, listed);

		nvim.set_var("sarthi_test", (true, 3)).unwrap();
		let value: (bool, u32) = nvim.exec_lua("return vim.g.sarthi_test", ()).unwrap();
		assert_eq!(value, (true, 3));
	}

	#[test]
	fn asking_to_quit_asks_about_unsaved_changes() {
		use std::{thread, time::Duration};

		use crate::{UiAttachParams, UiOptions};

		// With a UI, as sarthi is: without one, Neovim answers its own question.
		let (mut nvim, _messages) = Nvim::start(OsStr::new("nvim"), &["--clean", "-i", "NONE", "-n"], None).unwrap();
		let options = UiOptions { ext_linegrid: true, ..UiOptions::default() };
		nvim.ui_attach(UiAttachParams { width: 40, height: 10, options }).unwrap();
		nvim.exec_lua::<()>("vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'unsaved' })", ())
			.unwrap();

		// It asks instead of quitting, and stays after "Cancel".
		nvim.ask_to_quit().unwrap();
		thread::sleep(Duration::from_millis(300));
		assert!(nvim.process.0.try_wait().unwrap().is_none(), "Neovim quit with unsaved changes");
		nvim.input("c").unwrap();
		thread::sleep(Duration::from_millis(100));
		assert!(nvim.process.0.try_wait().unwrap().is_none());

		// Nothing unsaved: it quits.
		nvim.exec_lua::<()>("vim.bo.modified = false", ()).unwrap();
		nvim.ask_to_quit().unwrap();
		let start = Instant::now();
		while nvim.process.0.try_wait().unwrap().is_none() {
			assert!(start.elapsed() < Duration::from_secs(2), "Neovim didn't quit");
			thread::sleep(Duration::from_millis(20));
		}
	}
}
