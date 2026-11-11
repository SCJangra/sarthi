use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

use editor::{Editor, Pointer, Redraw};
use renderer::{CellSize, FontChoice, FontId, Fonts, Frame, Renderer, RendererConfig, RendererError, TextSize};
use rpc::client::{IncomingRequest, Message, Nvim};
use rpc::neovim::{
	ClientInfo, ClientVersion, InputMouseParams, Notification, RedrawNotification, UiAttachParams, UiOptions,
	UiTryResizeParams,
};
use rpc::sarthi::{
	ChangeFontSizeArgs, Copied, FloatsChangedArgs, GetClipboardArgs, MODULES, Request, ResetFontSizeArgs, STARTUP,
	SetClipboardArgs, SetFontSizeArgs, Settings, SetupArgs, WinScrolledArgs, WindowOptions, WindowVariable,
};

use crate::blur::Blur;
use crate::clipboard::Clipboard;
use crate::error::Error;
use crate::guifont::{self, FontSizes, GuiFont};
use crate::input::neovim_keys;
use crate::mouse::Mouse;
use crate::settings;
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

	/// What the font size functions changed `guifont` from, for a reset
	pub font_sizes: FontSizes,

	/// The `linespace` value last applied, in logical pixels
	pub linespace: i64,

	pub nvim: Nvim,

	/// Grid size last sent to Neovim, so resizes that don't change it aren't sent again
	pub ui_size: GridSize,

	/// Modifier keys held right now, for the keys and mouse events sent to Neovim
	pub modifiers: ModifiersState,

	/// What the mouse is doing over the window
	pub mouse: Mouse,

	/// How the pointer looks over the window, as last set
	pub pointer: Pointer,

	/// What the user set with `require("sarthi").setup()`, or the defaults
	pub settings: Settings,

	/// The mistake last reported in each window's `w:sarthi`, so each is reported once
	pub window_mistakes: HashMap<rpc::neovim::Window, String>,

	pub error: Option<Error>,
}

/// A window and its renderer, which holds what the window shows
pub struct WindowState {
	/// The clipboard of the window's display, if it could connect. Dropped first: it needs the display.
	pub clipboard: Option<Clipboard>,

	/// The blur behind the window, which also needs the display
	pub blur: Blur,

	pub window: Arc<Window>,
	pub renderer: Renderer,
}

#[derive(Debug)]
pub enum AppEvent {
	/// A notification or request from Neovim
	Nvim(Message),

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
			mouse: Mouse::new(Settings::default().touchpad.scroll_speed),
			pointer: Pointer::Arrow,
			settings: Settings::default(),
			window_mistakes: HashMap::new(),
			config,
			window: None,
			editor: Editor::new(),
			fonts: Fonts::new(),
			guifont: String::new(),
			font_sizes: FontSizes::default(),
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
			// So `window.opacity` can make the background see-through: decided here, before the user's config runs
			.with_transparent(true)
	}

	fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<Arc<Window>, Error> {
		let window = Arc::new(event_loop.create_window(Self::window_attributes(&self.config))?);
		let renderer = Self::create_renderer(&window, self.editor.background(), &self.settings)?;
		let clipboard = Clipboard::connect(&window)
			.inspect_err(|error| tracing::warn!("Couldn't connect to the clipboard: {error}"))
			.ok();
		let mut blur = Blur::new(window.clone());
		blur.set(self.settings.window.blur);
		self.window = Some(WindowState { clipboard, blur, window: window.clone(), renderer });
		self.update_float_styles();
		self.update_text_size();
		Ok(window)
	}

	fn create_renderer(window: &Arc<Window>, background: u32, settings: &Settings) -> Result<Renderer, Error> {
		let size = window.inner_size();
		let config = RendererConfig {
			background,
			scroll: settings::scroll_motion(settings),
			cursor: settings::cursor_motion(settings),
			opacity: settings.window.opacity,
			..RendererConfig::default()
		};
		Ok(smol::block_on(Renderer::with_config(window.clone(), size.width, size.height, config))?)
	}

	/// Size of text in `window`, in physical pixels
	fn text_size(&self, window: &Window) -> TextSize {
		let scale = window.scale_factor();
		// Logical pixels like the font size, in whole physical ones
		let line_space = (self.linespace as f64 * scale)
			.round()
			.clamp(f64::from(i32::MIN), f64::from(i32::MAX));
		let underlines = settings::underlines(&self.settings).scaled(scale as f32);
		TextSize { font_size: self.font_size * scale as f32, line_space: line_space as i32, underlines }
	}

	/// Size of a grid cell, in physical pixels: of the main fonts, as last sized (see [`App::update_text_size`])
	fn cell_size(&self) -> CellSize {
		self.fonts.cell_size(FontId::MAIN)
	}

	/// Number of grid cells that fit in `window`
	fn grid_size(&self, window: &Window) -> GridSize {
		GridSize::fit(window.inner_size(), self.cell_size())
	}

	/// Sizes the main fonts' text for the window, and tells the editor the size of its cells, which places its layers:
	/// after the font size, line space, underlines or scale changed
	fn update_text_size(&mut self) {
		if let Some(state) = &self.window {
			let size = self.text_size(&state.window);
			self.fonts.set_size(FontId::MAIN, size);
			self.editor.set_cell_size(self.cell_size());
		}
	}

	fn attach_ui(&mut self, window: &Window) -> Result<(), Error> {
		let size = self.grid_size(window);

		// With multigrid, Neovim draws each window to its own grid, and sarthi places and stacks them.
		let options = UiOptions::all();
		let params = UiAttachParams { width: size.width, height: size.height, options };

		// Names sarthi's channel, so plugins can tell it's sarthi.
		self.nvim.set_client_info(client_info())?;

		// All before attaching, so before the user's config, which Neovim sources once a UI attached. A config can then
		// check for sarthi (`if vim.g.sarthi then … end`) and use `require("sarthi")`.
		self.nvim.set_var("sarthi", true)?;
		let channel = self.nvim.channel()?;
		if let Err(error) = self.nvim.exec_lua::<()>(STARTUP, (channel, MODULES)) {
			tracing::warn!("Couldn't set up sarthi's Lua: {error}");
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

		let params = UiTryResizeParams { width: size.width, height: size.height };

		self.nvim.ui_try_resize(params)?;
		self.ui_size = size;
		Ok(())
	}

	fn handle_nvim_notification(&mut self, notification: Notification) {
		match notification {
			Notification::Redraw(events) => self.redraw_nvim(events),
			// Before the redraw that scrolls them
			Notification::WinScrolled(WinScrolledArgs { windows }) => {
				for window in windows {
					let options = self.window_options(window.win, &window.sarthi);
					let motion = settings::window_scroll(&self.settings, &window, &options);
					self.editor.set_scroll_motion(window.win, motion);
				}
			},
			// Before Neovim draws them
			Notification::FloatsChanged(FloatsChangedArgs { windows }) => {
				for window in windows {
					let options = self.window_options(window.win, &window.sarthi);
					let style = settings::float_style(&self.settings, &window, &options);
					self.editor.set_float_style(window.win, style);
				}
				// E.g. a float made transparent: Neovim drew it already, and won't again for this.
				self.refresh();
			},
			Notification::Other { .. } => (),
		}
	}

	/// Shows the pointer's look for what's under it in `window`, if it changed
	fn update_pointer(&mut self, window: &Window) {
		let Some(pointer) = self.mouse.pointer(&self.editor) else { return };
		if pointer != self.pointer {
			self.pointer = pointer;
			window.set_cursor(match pointer {
				Pointer::Arrow => CursorIcon::Default,
				Pointer::Text => CursorIcon::Text,
				Pointer::ResizeColumns => CursorIcon::ColResize,
				Pointer::ResizeRows => CursorIcon::RowResize,
			});
		}
	}

	/// Gives the editor how floating windows look until sarthi learns about them, and the scale of the window's display,
	/// which their styles are drawn at
	fn update_float_styles(&mut self) {
		let scale = self.window.as_ref().map_or(1.0, |state| state.window.scale_factor()) as f32;
		self.editor.set_scale(scale);
		self.editor
			.set_default_float_style(settings::default_float_style(&self.settings));
	}

	/// What window `win`'s `w:sarthi` gives it. A mistake in it is shown in Neovim, once, and the window gets nothing of
	/// its own until it's fixed.
	fn window_options(&mut self, win: rpc::neovim::Window, variable: &WindowVariable) -> WindowOptions {
		match settings::window_options(&self.settings, variable) {
			Ok(options) => {
				self.window_mistakes.remove(&win);
				options
			},
			Err(mistake) => {
				if self.window_mistakes.get(&win) != Some(&mistake) {
					let message = format!("sarthi: w:sarthi of window {}: {mistake}", win.0);
					if let Err(error) = self.nvim.show_error(&message) {
						tracing::warn!("Couldn't report a mistake in w:sarthi to Neovim: {error}");
					}
					self.window_mistakes.insert(win, mistake);
				}
				WindowOptions::default()
			},
		}
	}

	/// Applies Neovim's redraws, and shows the result once Neovim finished one, so events that arrive before the window
	/// redraws can't show half-applied.
	fn redraw_nvim(&mut self, events: Vec<RedrawNotification>) {
		let Some(redraw) = self.editor.apply(events) else { return };
		let fonts_changed = self.editor.guifont() != self.guifont;
		if fonts_changed {
			self.apply_guifont();
		}
		// Either can change the cell size, so how many cells fit in the window, and where the layers go.
		let mut redraw = redraw;
		if fonts_changed || self.editor.linespace() != self.linespace {
			self.linespace = self.editor.linespace();
			self.update_text_size();
			let window = self.window.as_ref().map(|state| state.window.clone());
			if let Some(Err(error)) = window.map(|window| self.resize_nvim_ui(&window)) {
				tracing::warn!("Couldn't resize Neovim's grid to the new cell size: {error}");
			}
			redraw = self.editor.refresh().unwrap_or(redraw);
		}

		self.show(redraw);
	}

	/// Shows what changed on sarthi's side, like a float's shadow, without waiting for Neovim's next redraw
	fn refresh(&mut self) {
		if let Some(redraw) = self.editor.refresh() {
			self.show(redraw);
		}
	}

	fn show(&mut self, redraw: Redraw) {
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
	fn handle_nvim_request(&mut self, IncomingRequest { request, responder }: IncomingRequest) {
		match request {
			// A mistake isn't a failed request: setup() raises it in the user's config, at the call.
			Request::Setup(SetupArgs { settings }) => {
				responder.respond(Ok::<_, String>(self.apply_settings(*settings).err()))
			},
			Request::SetClipboard(SetClipboardArgs { register, lines, regtype }) => {
				let copied = Copied { lines, regtype };
				responder.respond(self.clipboard().and_then(|clipboard| clipboard.copy(register, copied)));
			},
			Request::GetClipboard(GetClipboardArgs { register }) => {
				responder.respond(self.clipboard().and_then(|clipboard| clipboard.paste(register)));
			},
			Request::SetFontSize(SetFontSizeArgs { guifont, points }) => {
				let guifont = self.editor.strip_default_guifont(&guifont);
				responder.respond(self.font_sizes.set(guifont, points));
			},
			Request::ChangeFontSize(ChangeFontSizeArgs { guifont, delta }) => {
				let guifont = self.editor.strip_default_guifont(&guifont);
				let default = guifont::points(self.config.font_size);
				responder.respond(self.font_sizes.change(guifont, delta, default));
			},
			Request::ResetFontSize(ResetFontSizeArgs { guifont }) => {
				let guifont = self.editor.strip_default_guifont(&guifont);
				responder.respond(Ok::<_, String>(self.font_sizes.reset(guifont)));
			},
			Request::Other { method, .. } => responder.respond::<()>(Err(format!("sarthi doesn't know {method}"))),
		}
	}

	/// The clipboard of the window's display
	fn clipboard(&mut self) -> Result<&mut Clipboard, String> {
		let clipboard = self.window.as_mut().and_then(|state| state.clipboard.as_mut());
		clipboard.ok_or_else(|| String::from("no clipboard"))
	}

	/// Uses `settings` from now on, if their values make sense. Text drawn with other underlines is laid out again.
	fn apply_settings(&mut self, settings: Settings) -> Result<(), String> {
		settings.validate()?;
		self.mouse.set_touchpad_scroll(settings.touchpad.scroll_speed);
		self.editor
			.set_cursor_animation(settings.cursor.animate_insert_mode, settings.cursor.animate_cmdline);
		self.editor.set_see_through(settings.window.opacity < 1.0);
		if let Some(state) = &mut self.window {
			state.renderer.set_scroll_motion(settings::scroll_motion(&settings));
			state.renderer.set_cursor_motion(settings::cursor_motion(&settings));
			state.renderer.set_opacity(settings.window.opacity);
			// The compositor blurs the desktop behind the window, where it can.
			state.blur.set(settings.window.blur);
			state.window.request_redraw();
		}
		self.settings = settings;
		self.update_float_styles();
		// The underlines' sizes are the text's.
		self.update_text_size();
		self.refresh();
		Ok(())
	}

	fn resize_renderer(&mut self) {
		if let Some(state) = &mut self.window {
			let size = state.window.inner_size();
			state.renderer.resize(size.width, size.height);
		}
	}

	fn redraw(&mut self) -> Result<(), Error> {
		let background = self.editor.background();
		let Some(state) = &mut self.window else { return Ok(()) };

		// Lets winit pace redraws to the display on Wayland (frame callbacks), so frames aren't drawn faster than
		// they can be shown: with FIFO presentation, the extra frames queued up and made resizing lag.
		let window = &state.window;
		let frame = state.renderer.render(&mut self.fonts, || window.pre_present_notify());

		match frame {
			// Text that slides after scrolling moves on in the next frame.
			Ok(Frame::Presented) if state.renderer.is_animating() => state.window.request_redraw(),
			Ok(Frame::Presented | Frame::Skipped) => (),
			Ok(Frame::Reconfigured) => state.window.request_redraw(),
			Err(RendererError::SurfaceLost) => {
				let layers = state.renderer.layers().to_vec();
				state.renderer = Self::create_renderer(&state.window, background, &self.settings)?;
				state.renderer.set_layers(layers);
				state.window.request_redraw();
			},
			Err(err) => tracing::error!("Redraw renderer error: {err:?}"),
		}

		Ok(())
	}

	/// Sends mouse events to Neovim. Errors mean the connection closed, after which the app exits anyway.
	fn send_mouse(&self, events: impl IntoIterator<Item = InputMouseParams>) {
		for event in events {
			if let Err(error) = self.nvim.input_mouse(event) {
				tracing::warn!("Couldn't send a mouse event to Neovim: {error}");
			}
		}
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
				self.update_float_styles();
				self.update_text_size();
				self.refresh();
				if let Err(error) = self.resize_nvim_ui(&window) {
					self.exit_with_error(event_loop, error);
					return;
				}
				// The text size changed, even if the grid size didn't.
				window.request_redraw();
			},
			WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
			WindowEvent::CursorMoved { position, .. } => {
				let event = self.mouse.moved(position, &self.editor, self.modifiers);
				self.send_mouse(event);
				self.update_pointer(&window);
			},
			WindowEvent::CursorLeft { .. } => self.mouse.left(),
			WindowEvent::MouseInput { state, button, .. } => {
				let event = self.mouse.button(button, state, &self.editor, self.modifiers);
				self.send_mouse(event);
				// A drag may end over something else.
				self.update_pointer(&window);
			},
			WindowEvent::MouseWheel { delta, .. } => {
				let events = self.mouse.wheel(delta, &self.editor, self.cell_size(), self.modifiers);
				self.send_mouse(events);
			},
			// Synthetic presses are keys already held when the window gained focus.
			WindowEvent::KeyboardInput { event, is_synthetic: false, .. } => self.send_key(&event),
			WindowEvent::Focused(focused) => {
				// The cursor shows whether keys go here.
				self.editor.set_focused(focused);
				self.refresh();
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
			AppEvent::Nvim(Message::Notification(notification)) => self.handle_nvim_notification(notification),
			AppEvent::Nvim(Message::Request(request)) => self.handle_nvim_request(request),
			AppEvent::NvimExited => event_loop.exit(),
		}
	}
}

/// How sarthi introduces itself to Neovim (`:help nvim_set_client_info()`)
fn client_info() -> ClientInfo {
	let number = |part: &str| part.parse().unwrap_or(0);
	let version = ClientVersion {
		major: number(env!("CARGO_PKG_VERSION_MAJOR")),
		minor: number(env!("CARGO_PKG_VERSION_MINOR")),
		patch: number(env!("CARGO_PKG_VERSION_PATCH")),
	};
	ClientInfo { name: String::from("sarthi"), version, kind: "ui" }
}
