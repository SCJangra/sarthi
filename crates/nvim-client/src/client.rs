use std::time::{Duration, Instant};
use std::{process, thread};

use flume::Receiver;

use crate::{
	Error, NvimInput, NvimNotification, NvimUiAttach, NvimUiAttachParams, NvimUiSetFocus, NvimUiTryResize,
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
	pub fn start() -> Result<Self, Error> {
		Self::spawn(&["--embed"])
	}

	/// Starts Neovim with `args`, which must include `--embed`
	fn spawn(args: &[&str]) -> Result<Self, Error> {
		let mut nvim = process::Command::new("nvim")
			.args(args)
			.stdin(process::Stdio::piped())
			.stdout(process::Stdio::piped())
			.spawn()
			.map_err(Error::SpawnNvim)?;

		let stdin = nvim.stdin.take().expect("stdin handle present");
		let stdout = nvim.stdout.take().expect("stdout handle present");

		let rpc = RpcClient::start(stdout, stdin);

		Ok(Self { rpc, process: NvimProcess(nvim) })
	}

	pub fn notifications(&self) -> Result<Receiver<NvimNotification>, Error> {
		self.rpc.subscribe().map_err(Error::Rpc)
	}

	pub async fn ui_attach(&self, params: NvimUiAttachParams) -> Result<rmpv::Value, Error> {
		self.rpc.call::<NvimUiAttach>(params).await.map_err(Error::Rpc)
	}

	pub async fn ui_try_resize(&self, params: NvimUiTryResizeParams) -> Result<rmpv::Value, Error> {
		self.rpc.call::<NvimUiTryResize>(params).await.map_err(Error::Rpc)
	}

	/// Types `keys`, in `:help key-notation`, without waiting for Neovim to process them
	pub fn input(&self, keys: &str) -> Result<(), Error> {
		self.rpc.notify::<NvimInput>([keys.to_owned()]).map_err(Error::Rpc)
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
		let nvim = Nvim::spawn(&["--embed", "--clean", "-i", "NONE", "-n"]).unwrap();
		let pid = nvim.process.0.id();

		let start = Instant::now();
		drop(nvim);

		// Exiting on its own (not killed after the timeout), and reaped: no zombie left in /proc.
		assert!(start.elapsed() < EXIT_TIMEOUT, "Neovim had to be killed");
		assert!(!Path::new(&format!("/proc/{pid}")).exists(), "Neovim process {pid} still exists");
	}
}
