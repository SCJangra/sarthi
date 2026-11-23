//! Sarthi's API: the requests of `require('sarthi')`, answered on a thread pool so the main thread never waits on
//! Neovim.
//!
//! Each request runs on a pool thread, which calls Neovim as it needs to, blocking only itself; Neovim waits for the
//! answer meanwhile, but still answers Sarthi's calls. What only the main thread can do (the settings, fonts, the
//! editor, the clipboard) runs there as a [`MainTask`], and the request waits for its result. Those tasks are queued
//! before the request is answered, so whatever they change is in place before Neovim redraws.

mod window;

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use rpc::client::{Error as NvimError, IncomingRequest, NvimHandle, RpcError};
use rpc::neovim::Window;
use rpc::sarthi::{
	ChangeFontSizeArgs, Copied, GetClipboardArgs, NvimOpenWinArgs, NvimWinGetConfigArgs, NvimWinSetConfigArgs, Request,
	SetClipboardArgs, SetFontSizeArgs, Settings, SetupArgs,
};
use winit::event_loop::EventLoopProxy;

use crate::app::{App, AppEvent};
use crate::guifont;

/// Work only the main thread can do, which owns the window, the editor, the fonts and the settings.
pub struct MainTask(Box<dyn FnOnce(&mut App) + Send>);

impl MainTask {
	/// Does the work, on the main thread.
	pub fn run(self, app: &mut App) {
		(self.0)(app);
	}
}

impl fmt::Debug for MainTask {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str("MainTask")
	}
}

/// What a request works with: Neovim, the main thread, and what the API keeps about windows. Cheap to clone, one for
/// each request.
#[derive(Clone)]
pub struct Ctx {
	nvim: NvimHandle,
	main: EventLoopProxy<AppEvent>,

	/// `sarthi.place` and `sarthi.font` as each window was last given them, which aren't in its `w:sarthi`.
	given: Arc<Mutex<HashMap<Window, window::Given>>>,
}

impl Ctx {
	/// A context calling Neovim through `nvim`, and the main thread through `main`.
	pub fn new(nvim: NvimHandle, main: EventLoopProxy<AppEvent>) -> Self {
		Self { nvim, main, given: Arc::default() }
	}

	/// Forgets what the API kept about `win`, which closed.
	pub fn forget(&self, win: Window) {
		self.given().remove(&win);
	}

	fn given(&self) -> std::sync::MutexGuard<'_, HashMap<Window, window::Given>> {
		self.given.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Runs `task` on the main thread, and waits for its result.
	fn on_main<R: Send + 'static>(&self, task: impl FnOnce(&mut App) -> R + Send + 'static) -> Result<R, String> {
		let (reply, result) = flume::bounded(1);
		let task = MainTask(Box::new(move |app| _ = reply.send(task(app))));
		self.main.send_event(AppEvent::Main(task)).map_err(|_| closing())?;
		result.recv().map_err(|_| closing())
	}
}

/// Answers `request`, on the calling thread: one of the pool's.
pub fn handle(ctx: &Ctx, IncomingRequest { request, responder }: IncomingRequest) {
	match request {
		Request::Setup(SetupArgs { settings }) => {
			responder.respond(setup(ctx, settings.map_or_else(Settings::default, |settings| *settings)))
		},
		Request::GetSettings(_) => responder.respond(ctx.on_main(|app| app.settings.clone())),
		Request::SetFontSize(SetFontSizeArgs { points }) => {
			responder.respond(set_guifont(ctx, move |_, value| {
				guifont::parse(value)?;
				Ok(guifont::with_size(value, guifont::checked(points)?))
			}));
		},
		Request::ChangeFontSize(ChangeFontSizeArgs { delta }) => {
			responder.respond(set_guifont(ctx, move |app, value| {
				guifont::parse(value)?;
				app.change_window_font_sizes(delta);
				Ok(guifont::with_size(value, guifont::changed(app.main_font().1, delta)))
			}));
		},
		Request::NvimOpenWin(NvimOpenWinArgs { buf, enter, config }) => {
			responder.respond(window::open(ctx, buf, enter, config));
		},
		Request::NvimWinSetConfig(NvimWinSetConfigArgs { win, config }) => {
			responder.respond(window::set_config(ctx, win, config));
		},
		Request::NvimWinGetConfig(NvimWinGetConfigArgs { win }) => responder.respond(window::get_config(ctx, win)),
		Request::SetClipboard(SetClipboardArgs { register, lines, regtype }) => {
			let copied = Copied { lines, regtype };
			responder.respond(
				ctx.on_main(move |app| app.clipboard()?.copy(register, copied))
					.and_then(|copied| copied),
			);
		},
		Request::GetClipboard(GetClipboardArgs { register }) => {
			responder.respond(
				ctx.on_main(move |app| app.clipboard()?.paste(register))
					.and_then(|pasted| pasted),
			);
		},
		Request::Other { method, .. } => responder.respond::<()>(Err(format!("sarthi doesn't know {method}"))),
	}
}

/// Uses `settings` from now on, if their values make sense, and sends the floating windows open again, which get them
/// too.
fn setup(ctx: &Ctx, settings: Settings) -> Result<(), String> {
	let highlight = settings.window.outline.highlight.clone();
	ctx.on_main(move |app| app.apply_settings(settings))??;
	ctx.nvim
		.exec_lua::<()>("require('sarthi.floats').resend(...)", (highlight,))
		.map_err(neovim_error)
}

/// Sets `guifont` to what `size` makes of it, as it is now, with Neovim's default left out.
fn set_guifont(
	ctx: &Ctx,
	size: impl FnOnce(&mut App, &str) -> Result<String, String> + Send + 'static,
) -> Result<(), String> {
	let guifont = ctx.nvim.get_string_option("guifont").map_err(neovim_error)?;
	let guifont = ctx.on_main(move |app| {
		let guifont = app.editor.strip_default_guifont(&guifont).to_owned();
		size(app, &guifont).map(|guifont| app.editor.with_default_guifont(guifont))
	})??;
	ctx.nvim.set_string_option("guifont", guifont).map_err(neovim_error)
}

/// Neovim's message for an error it raised, or what else went wrong.
fn neovim_error(error: NvimError) -> String {
	match error {
		// Neovim's errors are `[type, message]`.
		NvimError::Rpc(RpcError::Response(rmpv::Value::Array(error))) => match error.as_slice() {
			[_, rmpv::Value::String(message)] => message.as_str().unwrap_or("Neovim raised an error").to_owned(),
			_ => String::from("Neovim raised an error"),
		},
		NvimError::Rpc(RpcError::RequestDropped | RpcError::SendRequest) => closing(),
		error => error.to_string(),
	}
}

/// Why a request couldn't be answered: sarthi or Neovim is closing.
fn closing() -> String {
	String::from("sarthi is closing")
}
