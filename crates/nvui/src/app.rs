use std::collections::HashSet;
use std::sync::Arc;
use std::thread;

use dashmap::DashMap;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoopProxy;
use winit::window::{Window, WindowAttributes, WindowId};

use nvim::{Nvim, NvimNotification, NvimUiAttachParams, NvimUiOptions, NvimUiTryResizeParams, RedrawNotification};
use renderer::{Frame, Renderer, RendererConfig, RendererError};

use crate::error::Error;
use crate::grid::Grid;
use crate::highlight::HighlightState;
use crate::windows::{GridSize, WindowConfig};

pub struct App {
	pub config: WindowConfig,
	pub window: DashMap<WindowId, Arc<Window>>,
	pub renderer: DashMap<WindowId, Renderer>,
	pub grid: DashMap<u32, Grid>,
	pub grid_window: DashMap<u32, WindowId>,
	pub highlight_state: HighlightState,
	pub nvim: Nvim,

	/// Grid size last sent to Neovim, so resizes that don't change it aren't sent again
	pub ui_size: GridSize,

	/// Windows showing stale content: their grids changed since the last `flush`, and they're redrawn on the
	/// next one
	pub stale_windows: HashSet<WindowId>,

	pub error: Option<Error>,
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
			window: DashMap::new(),
			renderer: DashMap::new(),
			grid: DashMap::new(),
			grid_window: DashMap::new(),
			highlight_state: HighlightState::new(),
			nvim,
			ui_size: GridSize { width: 0, height: 0 },
			stale_windows: HashSet::new(),
			error: None,
		})
	}

	fn window_attributes(config: &WindowConfig) -> WindowAttributes {
		Window::default_attributes()
			.with_title(config.title.clone())
			.with_inner_size(LogicalSize::new(f64::from(config.width), f64::from(config.height)))
			.with_resizable(config.resizable)
	}

	fn create_window(&mut self, event_loop: &ActiveEventLoop, config: &WindowConfig) -> Result<Arc<Window>, Error> {
		let attributes = Self::window_attributes(config);
		let window = event_loop.create_window(attributes)?;
		let window = Arc::new(window);
		self.window.insert(window.id(), window.clone());
		Ok(window)
	}

	fn create_renderer(&mut self, window: &Arc<Window>) -> Result<(), Error> {
		let size = window.inner_size();

		let renderer = smol::block_on(Renderer::with_config(
			window.clone(),
			size.width,
			size.height,
			RendererConfig { background: self.highlight_state.defaults().background, ..RendererConfig::default() },
		))?;

		self.renderer.insert(window.id(), renderer);

		Ok(())
	}

	/// Number of grid cells that fit in `window`
	fn grid_size(&self, window: &Window) -> GridSize {
		let cell_size = self.config.cell_size.to_physical(window.scale_factor());
		GridSize::fit(window.inner_size(), cell_size)
	}

	fn attach_ui(&mut self, window: &Window) -> Result<(), Error> {
		let size = self.grid_size(window);

		// TODO: Enable `ext_multigrid` once grids can be composited. With it, Neovim draws each window
		// to its own grid and positions them with `win_pos` and related events; without it,
		// everything is drawn to grid 1.
		let options = NvimUiOptions { ext_multigrid: false, ..NvimUiOptions::all() };
		let params = NvimUiAttachParams { width: size.width, height: size.height, options };

		smol::block_on(self.nvim.ui_attach(params))?;
		self.ui_size = size;
		self.grid.insert(1, Grid::new(0, 0));
		self.grid_window.insert(1, window.id());
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

	fn handle_nvim_notification(&mut self, notification: NvimNotification) {
		match notification {
			NvimNotification::Redraw(events) => self.apply_redraw_events(events),
			NvimNotification::Other { .. } => (),
		}
	}

	fn apply_redraw_events(&mut self, events: Vec<RedrawNotification>) {
		for event in events {
			match event {
				RedrawNotification::GridResize(resizes) => {
					for resize in resizes {
						let Some(mut grid) = self.grid.get_mut(&resize.grid) else {
							tracing::warn!("GridResize: Unknown grid {}", resize.grid);
							continue;
						};
						grid.resize(resize.width as usize, resize.height as usize);
						drop(grid);
						self.mark_stale(resize.grid);
					}
				},
				RedrawNotification::GridClear(clears) => {
					for clear in clears {
						let Some(mut grid) = self.grid.get_mut(&clear.grid) else {
							tracing::warn!("GridClear: Unknown grid {}", clear.grid);
							continue;
						};
						grid.clear();
						drop(grid);
						self.mark_stale(clear.grid);
					}
				},
				RedrawNotification::GridLine(lines) => {
					for line in lines {
						let Some(mut grid) = self.grid.get_mut(&line.grid) else {
							tracing::warn!("GridLine: Unknown grid {}", line.grid);
							continue;
						};
						grid.set_line(line.row as usize, line.col_start as usize, &line.cells);
						drop(grid);
						self.mark_stale(line.grid);
					}
				},
				RedrawNotification::GridCursorGoto(cursors) => {
					for cursor in cursors {
						let Some(mut grid) = self.grid.get_mut(&cursor.grid) else {
							tracing::warn!("GridCursorGoto: Unknown grid {}", cursor.grid);
							continue;
						};
						grid.set_cursor(cursor.row as usize, cursor.col as usize);
						drop(grid);
						self.mark_stale(cursor.grid);
					}
				},
				RedrawNotification::HlAttrDefine(attrs) => {
					self.highlight_state.set_attrs(attrs);
					self.mark_all_stale();
				},
				RedrawNotification::DefaultColorsSet(colors) => {
					self.highlight_state.set_default_colors(colors);
					let background = self.highlight_state.defaults().background;
					for mut renderer in self.renderer.iter_mut() {
						renderer.set_clear_color(background);
					}
					self.mark_all_stale();
				},
				RedrawNotification::HlGroupSet(groups) => {
					self.highlight_state.set_hl_groups(groups);
				},
				RedrawNotification::Flush => self.flush(),
				_ => (),
			}
		}
	}

	/// Marks the window showing `grid` as stale, so it's redrawn on the next `flush`
	fn mark_stale(&mut self, grid: u32) {
		let Some(window_id) = self.grid_window.get(&grid).map(|entry| *entry.value()) else {
			tracing::error!("Unknown window for grid {grid}");
			return;
		};
		self.stale_windows.insert(window_id);
	}

	/// Marks every window as stale, e.g. after highlights changed
	fn mark_all_stale(&mut self) {
		self.stale_windows.extend(self.window.iter().map(|entry| *entry.key()));
	}

	/// Neovim is done redrawing, so its state is consistent: redraw the windows that changed.
	fn flush(&mut self) {
		for window_id in self.stale_windows.drain() {
			if let Some(window) = self.window.get(&window_id) {
				window.request_redraw();
			}
		}
	}

	fn resize_renderer(&mut self, id: WindowId, window: &Window) -> Result<(), Error> {
		let Some(mut renderer) = self.renderer.get_mut(&id) else { return Ok(()) };
		let size = window.inner_size();

		renderer.resize(size.width, size.height);
		Ok(())
	}

	fn redraw_renderer(&mut self, id: WindowId, window: &Arc<Window>) -> Result<(), Error> {
		let Some(mut renderer) = self.renderer.get_mut(&id) else { return Ok(()) };
		let frame = renderer.render_clear();
		drop(renderer);

		match frame {
			Ok(Frame::Presented | Frame::Skipped) => (),
			Ok(Frame::Reconfigured) => window.request_redraw(),
			Err(RendererError::SurfaceLost) => {
				self.create_renderer(window)?;
				window.request_redraw();
			},
			Err(err) => tracing::error!("Redraw renderer error: {err:?}"),
		}

		Ok(())
	}

	/// Records `error` and exits. Only the first error is kept, so a later failure can't hide the
	/// original cause.
	fn exit_with_error(&mut self, event_loop: &ActiveEventLoop, error: Error) {
		self.error.get_or_insert(error);
		event_loop.exit();
	}

	fn close_window(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
		self.renderer.remove(&id);
		self.window.remove(&id);
		self.stale_windows.remove(&id);

		if self.window.is_empty() {
			self.grid.clear();
			self.grid_window.clear();
			event_loop.exit();
		}
	}
}

impl ApplicationHandler<AppEvent> for App {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if !self.window.is_empty() {
			return;
		}

		let config = self.config.clone();
		let window = match self.create_window(event_loop, &config) {
			Ok(window) => window,
			Err(error) => {
				self.exit_with_error(event_loop, error);
				return;
			},
		};

		if let Err(error) = self.create_renderer(&window) {
			self.exit_with_error(event_loop, error);
			return;
		}

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

		#[rustfmt::skip]
		let Some(window) = self
			.window
			.get(&window_id)
			.map(|entry| entry.value().clone())
			 else { return };

		match event {
			WindowEvent::CloseRequested => {
				self.close_window(event_loop, window_id);
			},
			WindowEvent::Resized(_) => {
				if let Err(error) = self.resize_renderer(window_id, &window) {
					self.exit_with_error(event_loop, error);
					return;
				}
				if let Err(error) = self.resize_nvim_ui(&window) {
					self.exit_with_error(event_loop, error);
				}
			},
			WindowEvent::ScaleFactorChanged { .. } => {
				if let Err(error) = self.resize_renderer(window_id, &window) {
					self.exit_with_error(event_loop, error);
					return;
				}
				if let Err(error) = self.resize_nvim_ui(&window) {
					self.exit_with_error(event_loop, error);
				}
			},
			WindowEvent::RedrawRequested => {
				if let Err(error) = self.redraw_renderer(window_id, &window) {
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
