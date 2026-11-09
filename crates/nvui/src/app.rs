use std::sync::Arc;
use std::thread;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

use editor::Editor;
use nvim::{
	Nvim, NvimClientInfo, NvimClientVersion, NvimMessage, NvimNotification, NvimRequest, NvimUiAttachParams,
	NvimUiOptions, NvimUiTryResizeParams,
};
use renderer::{FontChoice, Fonts, Frame, Renderer, RendererConfig, RendererError, TextSize};

use crate::clipboard::{self, Clipboard, Copied, Register};
use crate::error::Error;
use crate::guifont::GuiFont;
use crate::input::neovim_keys;
use crate::windows::{GridSize, WindowConfig};

pub struct App {
	pub config: WindowConfig,

	/// The window, once created
	pub window: Option<WindowState>,

	pub editor: Editor,
	pub fonts: Fonts,

	/// Font size in logical pixels: from `guifont`, or the configured one
	pub font_size: f32,

	/// The `guifont` value last applied, so each value is applied (or reported as wrong) once
	pub guifont: String,

	/// The `linespace` value last applied, in logical pixels
	pub linespace: i64,

	pub nvim: Nvim,

	/// Grid size last sent to Neovim, so resizes that don't change it aren't sent again
	pub ui_size: GridSize,

	/// Modifier keys held right now, for the keys sent to Neovim
	pub modifiers: ModifiersState,

	pub error: Option<Error>,
}

/// A window and its renderer, which holds what the window shows
pub struct WindowState {
	/// The clipboard of the window's display, if it could connect. Dropped first: it needs the display.
	pub clipboard: Option<Clipboard>,

	pub window: Arc<Window>,
	pub renderer: Renderer,
}

#[derive(Debug)]
pub enum AppEvent {
	/// A notification or request from Neovim
	Nvim(NvimMessage),

	/// The connection to Neovim closed, e.g. after `:q`
	NvimExited,
}

impl App {
	pub fn new(config: WindowConfig, proxy: EventLoopProxy<AppEvent>) -> Result<Self, Error> {
		let (nvim, messages) = Nvim::start()?;
		let proxy = proxy.clone();

		thread::spawn(move || {
			for message in messages.iter() {
				let _ = proxy.send_event(AppEvent::Nvim(message));
			}

			// The message stream only ends once the connection to Neovim is closed.
			let _ = proxy.send_event(AppEvent::NvimExited);
		});

		Ok(Self {
			font_size: config.font_size,
			config,
			window: None,
			editor: Editor::new(),
			fonts: Fonts::new(),
			guifont: String::new(),
			linespace: 0,
			nvim,
			ui_size: GridSize { width: 0, height: 0 },
			modifiers: ModifiersState::empty(),
			error: None,
		})
	}

	fn window_attributes(config: &WindowConfig) -> WindowAttributes {
		Window::default_attributes()
			.with_title(config.title.clone())
			.with_inner_size(LogicalSize::new(f64::from(config.width), f64::from(config.height)))
			.with_resizable(config.resizable)
	}

	fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<Arc<Window>, Error> {
		let window = Arc::new(event_loop.create_window(Self::window_attributes(&self.config))?);
		let renderer = Self::create_renderer(&window, self.editor.background())?;
		let clipboard = Clipboard::connect(&window)
			.inspect_err(|error| tracing::warn!("Couldn't connect to the clipboard: {error}"))
			.ok();
		self.window = Some(WindowState { clipboard, window: window.clone(), renderer });
		Ok(window)
	}

	fn create_renderer(window: &Arc<Window>, background: u32) -> Result<Renderer, Error> {
		let size = window.inner_size();
		let config = RendererConfig { background, ..RendererConfig::default() };
		Ok(smol::block_on(Renderer::with_config(window.clone(), size.width, size.height, config))?)
	}

	/// Size of text in `window`, in physical pixels
	fn text_size(&self, window: &Window) -> TextSize {
		let scale = window.scale_factor();
		// Logical pixels like the font size, in whole physical ones
		let line_space = (self.linespace as f64 * scale)
			.round()
			.clamp(f64::from(i32::MIN), f64::from(i32::MAX));
		let underlines = self.config.underlines.scaled(scale as f32);
		TextSize { font_size: self.font_size * scale as f32, line_space: line_space as i32, underlines }
	}

	/// Number of grid cells that fit in `window`
	fn grid_size(&self, window: &Window) -> GridSize {
		let cell_size = self.fonts.cell_size(self.text_size(window));
		GridSize::fit(window.inner_size(), cell_size)
	}

	fn attach_ui(&mut self, window: &Window) -> Result<(), Error> {
		let size = self.grid_size(window);

		// With multigrid, Neovim draws each window to its own grid, and nvui places and stacks them.
		let options = NvimUiOptions::all();
		let params = NvimUiAttachParams { width: size.width, height: size.height, options };

		// Names nvui's channel, so Lua code (the clipboard below, plugins) can find it.
		self.nvim.set_client_info(client_info())?;

		// Before attaching, so it runs before the user's config, which Neovim sources once a UI attached.
		if let Err(error) = self.nvim.exec_lua(clipboard::SETUP) {
			tracing::warn!("Couldn't set up the clipboard: {error}");
		}
		self.nvim.ui_attach(params)?;
		self.ui_size = size;
		Ok(())
	}

	fn resize_nvim_ui(&mut self, window: &Window) -> Result<(), Error> {
		let size = self.grid_size(window);
		if size == self.ui_size {
			return Ok(());
		}

		let params = NvimUiTryResizeParams { width: size.width, height: size.height };

		self.nvim.ui_try_resize(params)?;
		self.ui_size = size;
		Ok(())
	}

	/// Applies Neovim's redraws, and shows the result once Neovim finished one, so events that arrive before the window
	/// redraws can't show half-applied.
	fn handle_nvim_notification(&mut self, notification: NvimNotification) {
		let NvimNotification::Redraw(events) = notification else { return };
		let Some(redraw) = self.editor.apply(events) else { return };
		let fonts_changed = self.editor.guifont() != self.guifont;
		if fonts_changed {
			self.apply_guifont();
		}
		// Either can change the cell size, so how many cells fit in the window.
		if fonts_changed || self.editor.linespace() != self.linespace {
			self.linespace = self.editor.linespace();
			let window = self.window.as_ref().map(|state| state.window.clone());
			if let Some(Err(error)) = window.map(|window| self.resize_nvim_ui(&window)) {
				tracing::warn!("Couldn't resize Neovim's grid to the new cell size: {error}");
			}
		}

		let Some(state) = &mut self.window else { return };
		state.renderer.set_clear_color(redraw.background);
		state.renderer.set_layers(redraw.layers);
		state.window.request_redraw();
	}

	/// Draws text with the fonts and size `guifont` asks for. A value that can't be used is shown as an error in Neovim,
	/// and the fonts stay as they were.
	fn apply_guifont(&mut self) {
		self.guifont = self.editor.guifont().to_owned();
		let applied = GuiFont::parse(&self.guifont).and_then(|font| {
			let pixels = font.pixels();
			let choice = FontChoice { families: font.families, bold: font.bold, italic: font.italic };
			self.fonts.choose(&choice).map_err(|error| error.to_string())?;
			Ok(pixels)
		});

		match applied {
			Ok(pixels) => self.font_size = pixels.unwrap_or(self.config.font_size),
			Err(error) => {
				if let Err(error) = self.nvim.show_error(&format!("guifont: {error}")) {
					tracing::warn!("Couldn't report a guifont error to Neovim: {error}");
				}
			},
		}
	}

	/// Answers a request from Neovim, which waits for the answer, so this must not wait for Neovim. Dropping a request
	/// answers it with an error.
	fn handle_nvim_request(&mut self, request: NvimRequest) {
		let clipboard = self
			.window
			.as_mut()
			.and_then(|state| state.clipboard.as_mut())
			.ok_or("no clipboard");
		let register = |name: &str| Register::from_name(name).ok_or_else(|| format!("no clipboard register {name}"));

		match request.method() {
			"nvui.set_clipboard" => {
				let copied = request
					.params::<(String, Vec<String>, String)>()
					.map_err(|error| error.to_string());
				let result = copied
					.and_then(|(name, lines, regtype)| clipboard?.copy(register(&name)?, Copied { lines, regtype }));
				request.respond(result);
			},
			"nvui.get_clipboard" => {
				let name = request.params::<(String,)>().map_err(|error| error.to_string());
				let result = name.and_then(|(name,)| clipboard?.paste(register(&name)?));
				request.respond(result);
			},
			_ => (),
		}
	}

	fn resize_renderer(&mut self) {
		if let Some(state) = &mut self.window {
			let size = state.window.inner_size();
			state.renderer.resize(size.width, size.height);
		}
	}

	fn redraw(&mut self) -> Result<(), Error> {
		let background = self.editor.background();
		let Some(size) = self.window.as_ref().map(|state| self.text_size(&state.window)) else { return Ok(()) };
		let Some(state) = &mut self.window else { return Ok(()) };

		// Lets winit pace redraws to the display on Wayland (frame callbacks), so frames aren't drawn faster than
		// they can be shown: with FIFO presentation, the extra frames queued up and made resizing lag.
		let window = &state.window;
		let frame = state.renderer.render(&mut self.fonts, size, || window.pre_present_notify());

		match frame {
			// Text that slides after scrolling moves on in the next frame.
			Ok(Frame::Presented) if state.renderer.is_animating() => state.window.request_redraw(),
			Ok(Frame::Presented | Frame::Skipped) => (),
			Ok(Frame::Reconfigured) => state.window.request_redraw(),
			Err(RendererError::SurfaceLost) => {
				let layers = state.renderer.layers().to_vec();
				state.renderer = Self::create_renderer(&state.window, background)?;
				state.renderer.set_layers(layers);
				state.window.request_redraw();
			},
			Err(err) => tracing::error!("Redraw renderer error: {err:?}"),
		}

		Ok(())
	}

	/// Types a key press in Neovim. Errors mean the connection closed, after which the app exits anyway.
	fn send_key(&self, event: &KeyEvent) {
		if event.state != ElementState::Pressed {
			return;
		}
		let Some(keys) = neovim_keys(&event.logical_key, event.text.as_deref(), event.location, self.modifiers) else {
			return;
		};
		if let Err(error) = self.nvim.input(&keys) {
			tracing::warn!("Couldn't send {keys} to Neovim: {error}");
		}
	}

	/// Records `error` and exits. Only the first error is kept, so a later failure can't hide the
	/// original cause.
	fn exit_with_error(&mut self, event_loop: &ActiveEventLoop, error: Error) {
		self.error.get_or_insert(error);
		event_loop.exit();
	}
}

impl ApplicationHandler<AppEvent> for App {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if self.window.is_some() {
			return;
		}

		let window = match self.create_window(event_loop) {
			Ok(window) => window,
			Err(error) => {
				self.exit_with_error(event_loop, error);
				return;
			},
		};

		if let Err(error) = self.attach_ui(&window) {
			self.exit_with_error(event_loop, error);
			return;
		}

		window.request_redraw();
	}

	fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
		if event_loop.exiting() {
			return;
		}

		let Some(window) = self.window.as_ref().map(|state| state.window.clone()) else { return };
		if window.id() != window_id {
			return;
		}

		match event {
			WindowEvent::CloseRequested => {
				self.window = None;
				event_loop.exit();
			},
			WindowEvent::Resized(_) => {
				self.resize_renderer();
				if let Err(error) = self.resize_nvim_ui(&window) {
					self.exit_with_error(event_loop, error);
				}
			},
			WindowEvent::ScaleFactorChanged { .. } => {
				self.resize_renderer();
				if let Err(error) = self.resize_nvim_ui(&window) {
					self.exit_with_error(event_loop, error);
					return;
				}
				// The text size changed, even if the grid size didn't.
				window.request_redraw();
			},
			WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
			// Synthetic presses are keys already held when the window gained focus.
			WindowEvent::KeyboardInput { event, is_synthetic: false, .. } => self.send_key(&event),
			WindowEvent::Focused(focused) => {
				if let Err(error) = self.nvim.ui_set_focus(focused) {
					tracing::warn!("Couldn't tell Neovim about focus: {error}");
				}
			},
			WindowEvent::RedrawRequested => {
				if let Err(error) = self.redraw() {
					self.exit_with_error(event_loop, error);
				}
			},
			_ => (),
		}
	}

	fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
		match event {
			AppEvent::Nvim(NvimMessage::Notification(notification)) => self.handle_nvim_notification(notification),
			AppEvent::Nvim(NvimMessage::Request(request)) => self.handle_nvim_request(request),
			AppEvent::NvimExited => event_loop.exit(),
		}
	}
}

/// How nvui introduces itself to Neovim (`:help nvim_set_client_info()`)
fn client_info() -> NvimClientInfo {
	let number = |part: &str| part.parse().unwrap_or(0);
	let version = NvimClientVersion {
		major: number(env!("CARGO_PKG_VERSION_MAJOR")),
		minor: number(env!("CARGO_PKG_VERSION_MINOR")),
		patch: number(env!("CARGO_PKG_VERSION_PATCH")),
	};
	NvimClientInfo { name: String::from("nvui"), version, kind: "ui" }
}
