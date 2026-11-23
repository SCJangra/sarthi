//! sarthi, a Neovim GUI: starts Neovim, opens the window, and runs the event loop.

mod api;
mod app;
mod args;
mod blur;
mod clipboard;
mod error;
mod guifont;
mod input;
mod mouse;
mod place;
mod settings;
mod startup;
mod windows;

use app::{App, AppEvent};
use args::Args;
use clap::Parser;
use windows::WindowConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let Args { nvim, args, .. } = Args::parse();

	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.init();

	let event_loop = winit::event_loop::EventLoop::<AppEvent>::with_user_event().build()?;
	let proxy = event_loop.create_proxy();
	let mut app = App::new(WindowConfig::default(), &nvim, &args, proxy)?;

	event_loop.run_app(&mut app)?;

	if app.nvim.is_err() {
		std::process::exit(1);
	}

	if let Some(error) = app.error.take() {
		return Err(Box::new(error));
	}

	Ok(())
}
