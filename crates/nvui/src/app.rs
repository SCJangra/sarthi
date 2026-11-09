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
use nvim::{Nvim, NvimNotification, NvimUiAttachParams, NvimUiOptions, NvimUiTryResizeParams};
use renderer::{Fonts, Frame, Renderer, RendererConfig, RendererError};

use crate::error::Error;
use crate::input::neovim_keys;
use crate::windows::{GridSize, WindowConfig};

pub struct App {
	pub config: WindowConfig,

	/// The window, once created
	pub window: Option<WindowState>,

	pub editor: Editor,
	pub fonts: Fonts,
	pub nvim: Nvim,

	/// Grid size last sent to Neovim, so resizes that don't change it aren't sent again
	pub ui_size: GridSize,

	/// Modifier keys held right now, for the keys sent to Neovim
	pub modifiers: ModifiersState,

	pub error: Option<Error>,
}

/// A window and its renderer, which holds what the window shows
pub struct WindowState {
	pub window: Arc<Window>,
	pub renderer: Renderer,
}

#[derive(Debug)]
pub enum AppEvent {
	Nvim(NvimNotification),

	/// The connection to Neovim closed, e.g. after `:q`
	NvimExited,
}

impl App {
	pub fn new(config: WindowConfig, proxy: EventLoopProxy<AppEvent>) -> Result<Self, Error> {
		let nvim = Nvim::start()?;
		let notifications = nvim.notifications()?;
		let proxy = proxy.clone();

		thread::spawn(move || {
			for notification in notifications.iter() {
				let _ = proxy.send_event(AppEvent::Nvim(notification));
			}

			// The notification stream only ends once the connection to Neovim is closed.
			let _ = proxy.send_event(AppEvent::NvimExited);
		});

		Ok(Self {
			config,
			window: None,
			editor: Editor::new(),
			fonts: Fonts::new(),
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
		self.window = Some(WindowState { window: window.clone(), renderer });
		Ok(window)
	}

	fn create_renderer(window: &Arc<Window>, background: u32) -> Result<Renderer, Error> {
		let size = window.inner_size();
		let config = RendererConfig { background, ..RendererConfig::default() };
		Ok(smol::block_on(Renderer::with_config(window.clone(), size.width, size.height, config))?)
	}

	/// Font size in `window`, in physical pixels
	fn font_size(&self, window: &Window) -> f32 {
		self.config.font_size * window.scale_factor() as f32
	}

	/// Number of grid cells that fit in `window`
	fn grid_size(&self, window: &Window) -> GridSize {
		let cell_size = self.fonts.cell_size(self.font_size(window));
		GridSize::fit(window.inner_size(), cell_size)
	}

	fn attach_ui(&mut self, window: &Window) -> Result<(), Error> {
		let size = self.grid_size(window);

		// With multigrid, Neovim draws each window to its own grid, and nvui places and stacks them.
		let options = NvimUiOptions::all();
		let params = NvimUiAttachParams { width: size.width, height: size.height, options };

		smol::block_on(self.nvim.ui_attach(params))?;
		self.ui_size = size;
		Ok(())
	}

	fn resize_nvim_ui(&mut self, window: &Window) -> Result<(), Error> {
		let size = self.grid_size(window);
		if size == self.ui_size {
			return Ok(());
		}

		let params = NvimUiTryResizeParams { width: size.width, height: size.height };

		smol::block_on(self.nvim.ui_try_resize(params))?;
		self.ui_size = size;
		Ok(())
	}

	/// Applies Neovim's redraws, and shows the result once Neovim finished one, so events that arrive before the window
	/// redraws can't show half-applied.
	fn handle_nvim_notification(&mut self, notification: NvimNotification) {
		let NvimNotification::Redraw(events) = notification else { return };
		let (Some(redraw), Some(state)) = (self.editor.apply(events), &mut self.window) else { return };

		state.renderer.set_clear_color(redraw.background);
		state.renderer.set_layers(redraw.layers);
		state.window.request_redraw();
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
		let font_size = self.config.font_size * state.window.scale_factor() as f32;

		// Lets winit pace redraws to the display on Wayland (frame callbacks), so frames aren't drawn faster than
		// they can be shown: with FIFO presentation, the extra frames queued up and made resizing lag.
		let window = &state.window;
		let frame = state
			.renderer
			.render(&mut self.fonts, font_size, || window.pre_present_notify());

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
			AppEvent::Nvim(notification) => self.handle_nvim_notification(notification),
			AppEvent::NvimExited => event_loop.exit(),
		}
	}
}
