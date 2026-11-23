//! Neovim: its process and the API functions sarthi calls.

use std::ffi::{OsStr, OsString};
use std::ops::Deref;
use std::time::{Duration, Instant};
use std::{process, thread};

use flume::Receiver;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{
	Buffer, CallFunction, ClientInfo, Error, ExecLua, GetChanInfo, GetCurrentWin, GetStringOption, Input, InputMouse,
	InputMouseParams, Message, NoOpts, OpenWin, RpcClient, RpcHandle, SetClientInfo, SetStringOption, SetVar, UiAttach,
	UiAttachParams, UiSetFocus, UiTryResize, UiTryResizeGrid, UiTryResizeGridParams, UiTryResizeParams, WinClose,
	WinGetConfig, WinSetConfig, WinSetVar, Window,
};

/// How long to wait for Neovim to exit on its own before killing it. Generous, since killing it
/// while it writes ShaDa leaves a temporary file behind.
const EXIT_TIMEOUT: Duration = Duration::from_secs(3);

/// Neovim, running embedded, and the API functions sarthi calls.
pub struct Nvim {
	/// What calls it, here and on other threads.
	handle: NvimHandle,

	// Fields drop in order: dropping `rpc` closes Neovim's input, which makes it exit, and then
	// `process` waits for that.
	#[expect(dead_code, reason = "kept for its drop, which closes Neovim's input")]
	rpc: RpcClient,
	#[allow(dead_code, reason = "kept for its drop, which waits for Neovim to exit")]
	process: NvimProcess,
}

impl Nvim {
	/// Starts Neovim, `program`, embedded, with `args` too, e.g. files to edit, in environment `env`, or sarthi's for
	/// `None`. Returns it and what it sends unasked (notifications, and requests from sarthi's Lua), which ends once
	/// Neovim exits.
	///
	/// # Errors
	///
	/// Fails if Neovim can't be started.
	pub fn start(
		program: &OsStr,
		args: &[impl AsRef<OsStr>],
		env: Option<&[(OsString, OsString)]>,
	) -> Result<(Self, Receiver<Message>), Error> {
		let args = [OsStr::new("--embed")].into_iter().chain(args.iter().map(AsRef::as_ref));
		Self::spawn(program, args, env)
	}

	/// Starts Neovim, `program`, with `args`, which must include `--embed`.
	fn spawn<'a>(
		program: &OsStr,
		args: impl IntoIterator<Item = &'a OsStr>,
		env: Option<&[(OsString, OsString)]>,
	) -> Result<(Self, Receiver<Message>), Error> {
		let mut command = process::Command::new(program);
		// Only what's given, e.g. the login shell's.
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

		Ok((Self { handle: NvimHandle { rpc: rpc.handle() }, rpc, process: NvimProcess(nvim) }, messages))
	}

	/// A handle that calls Neovim from any thread, cheap to clone. It doesn't keep Neovim running: once this is dropped,
	/// its calls fail.
	pub fn handle(&self) -> NvimHandle {
		self.handle.clone()
	}
}

/// Calls through the handle, here on the thread that owns Neovim.
impl Deref for Nvim {
	type Target = NvimHandle;

	fn deref(&self) -> &NvimHandle {
		&self.handle
	}
}

/// The API functions sarthi calls, from any thread (see [`Nvim::handle`]).
#[derive(Clone)]
pub struct NvimHandle {
	rpc: RpcHandle,
}

impl NvimHandle {
	/// The id of the channel sarthi and Neovim talk on, e.g. for Lua code that calls back with `rpcrequest()`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error.
	pub fn channel(&self) -> Result<u64, Error> {
		Ok(self.rpc.call::<GetChanInfo>([0]).map_err(Error::Rpc)?.id)
	}

	/// Sets the global variable `g:name` (`vim.g.name` in Lua) to `value`, without waiting for Neovim
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn set_var<V: Serialize>(&self, name: &str, value: V) -> Result<(), Error> {
		self.rpc.notify::<SetVar<V>>((name.to_owned(), value)).map_err(Error::Rpc)
	}

	/// Identifies the client to Neovim, e.g. for Lua code looking for its channel
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn set_client_info(&self, info: ClientInfo) -> Result<(), Error> {
		self.rpc.notify::<SetClientInfo>(info).map_err(Error::Rpc)
	}

	/// Shows `message` as an error in Neovim (`vim.notify()` at ERROR level), without waiting for it
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn show_error(&self, message: &str) -> Result<(), Error> {
		self.show("ERROR", message)
	}

	/// Shows `message` as a warning in Neovim (`vim.notify()` at WARN level), without waiting for it
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn show_warning(&self, message: &str) -> Result<(), Error> {
		self.show("WARN", message)
	}

	fn show(&self, level: &str, message: &str) -> Result<(), Error> {
		let code = format!("vim.notify(..., vim.log.levels.{level})");
		self.rpc.notify::<ExecLua>((code, vec![message.into()])).map_err(Error::Rpc)
	}

	/// Runs Lua `code` in Neovim with `args`, a tuple (`()` for none) the code gets as `...`, and waits for its result
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error Also if the result doesn't
	/// decode as `R`.
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
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error.
	pub fn ui_attach(&self, params: UiAttachParams) -> Result<rmpv::Value, Error> {
		self.rpc.call::<UiAttach>(params).map_err(Error::Rpc)
	}

	/// Asks Neovim to resize the UI, without waiting for it: the new size arrives with the redraws
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn ui_try_resize(&self, params: UiTryResizeParams) -> Result<(), Error> {
		self.rpc.notify::<UiTryResize>(params).map_err(Error::Rpc)
	}

	/// Asks Neovim to resize the window of a grid, without waiting for it: the new size arrives with the redraws
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn ui_try_resize_grid(&self, params: UiTryResizeGridParams) -> Result<(), Error> {
		self.rpc.notify::<UiTryResizeGrid>(params).map_err(Error::Rpc)
	}

	/// Types `keys`, in `:help key-notation`, without waiting for Neovim to process them
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn input(&self, keys: &str) -> Result<(), Error> {
		self.rpc.notify::<Input>([keys.to_owned()]).map_err(Error::Rpc)
	}

	/// Asks Neovim to quit as `:confirm qa` does, like gVim's close button: it asks about unsaved changes first, and
	/// may not quit. Typed as keys, so Neovim takes it in any mode, without waiting for it.
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn ask_to_quit(&self) -> Result<(), Error> {
		self.input("<Cmd>confirm qa<CR>")
	}

	/// Sends a mouse event, without waiting for Neovim to process it
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn input_mouse(&self, params: InputMouseParams) -> Result<(), Error> {
		self.rpc.notify::<InputMouse>(params).map_err(Error::Rpc)
	}

	/// Tells Neovim whether the UI has focus, for `FocusGained` and `FocusLost` autocommands
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn ui_set_focus(&self, focused: bool) -> Result<(), Error> {
		self.rpc.notify::<UiSetFocus>([focused]).map_err(Error::Rpc)
	}

	/// The current window
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn current_win(&self) -> Result<Window, Error> {
		self.rpc.call::<GetCurrentWin>([]).map_err(Error::Rpc)
	}

	/// Calls Vimscript function `name`, e.g. `winline`, with `args`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, Neovim raises an error, or the result doesn't decode
	/// as `R`.
	pub fn call_function<R: DeserializeOwned>(&self, name: &str, args: Vec<rmpv::Value>) -> Result<R, Error> {
		self.rpc.call::<CallFunction<R>>((name.to_owned(), args)).map_err(Error::Rpc)
	}

	/// Opens a window showing `buf`, with Neovim's window `config`, entering it if `enter`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error, e.g. about the config.
	pub fn open_win(&self, buf: Buffer, enter: bool, config: rmpv::Value) -> Result<Window, Error> {
		self.rpc.call::<OpenWin>((buf, enter, config)).map_err(Error::Rpc)
	}

	/// Closes `win`, even with changes not written if `force`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error.
	pub fn win_close(&self, win: Window, force: bool) -> Result<(), Error> {
		self.rpc.call::<WinClose>((win, force)).map_err(Error::Rpc)
	}

	/// `win`'s config, as Neovim gives it
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error.
	pub fn win_get_config(&self, win: Window) -> Result<rmpv::Value, Error> {
		self.rpc.call::<WinGetConfig>((win,)).map_err(Error::Rpc)
	}

	/// Changes `win`'s config to `config`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error, e.g. about the config.
	pub fn win_set_config(&self, win: Window, config: rmpv::Value) -> Result<(), Error> {
		self.rpc.call::<WinSetConfig>((win, config)).map_err(Error::Rpc)
	}

	/// `win`'s variable `w:name`, or nil if it has none
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error.
	pub fn win_get_var(&self, win: Window, name: &str) -> Result<rmpv::Value, Error> {
		self.call_function("getwinvar", vec![rmpv::Value::from(win.0), rmpv::Value::from(name), rmpv::Value::Nil])
	}

	/// Sets `win`'s variable `w:name` to `value`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error.
	pub fn win_set_var(&self, win: Window, name: &str, value: rmpv::Value) -> Result<(), Error> {
		self.rpc.call::<WinSetVar>((win, name.to_owned(), value)).map_err(Error::Rpc)
	}

	/// The value of string option `name`, e.g. `guifont`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error, e.g. for no such option.
	pub fn get_string_option(&self, name: &str) -> Result<String, Error> {
		self.rpc
			.call::<GetStringOption>((name.to_owned(), NoOpts {}))
			.map_err(Error::Rpc)
	}

	/// Sets string option `name`, e.g. `guifont`, to `value`
	///
	/// # Errors
	///
	/// Fails if the connection to Neovim closes before it answers, or Neovim raises an error, e.g. for a wrong value.
	pub fn set_string_option(&self, name: &str, value: String) -> Result<(), Error> {
		self.rpc
			.call::<SetStringOption>((name.to_owned(), value, NoOpts {}))
			.map_err(Error::Rpc)
	}

	/// Sets string option `name` to `value`, like [`NvimHandle::set_string_option`], without waiting for it. Neovim
	/// reports a wrong value as an error of its own.
	///
	/// # Errors
	///
	/// Fails once the connection to Neovim is closed.
	pub fn set_string_option_later(&self, name: &str, value: String) -> Result<(), Error> {
		self.rpc
			.notify::<SetStringOption>((name.to_owned(), value, NoOpts {}))
			.map_err(Error::Rpc)
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
