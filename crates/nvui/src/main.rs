mod app;
mod clipboard;
mod error;
mod guifont;
mod input;
mod mouse;
mod windows;

use app::{App, AppEvent};
use windows::WindowConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.init();

	let event_loop = winit::event_loop::EventLoop::<AppEvent>::with_user_event().build()?;
	let proxy = event_loop.create_proxy();
	let mut app = App::new(WindowConfig::default(), proxy)?;

	event_loop.run_app(&mut app)?;

	if let Some(error) = app.error.take() {
		return Err(Box::new(error));
	}

	Ok(())
}
