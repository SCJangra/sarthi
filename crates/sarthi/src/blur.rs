//! Blur behind the window, done by the compositor. On Wayland compositors that offer `ext-background-effect-v1`, like
//! current KDE Plasma, Sarthi asks for it itself: winit 0.30's `Window::set_blur` only speaks KDE's older blur protocol,
//! which KWin no longer offers. winit 0.31 adds the new one (rust-windowing/winit#4553); then `set_blur` is enough, and
//! the Wayland part can go (Vikunja #47).

use std::sync::Arc;

use winit::window::Window;

/// Blur behind a window: through `ext-background-effect` where the compositor offers it, otherwise winit's `set_blur`
/// (older KDE, macOS)
pub struct Blur {
	window: Arc<Window>,

	#[cfg(all(unix, not(target_os = "macos")))]
	effect: Option<wayland::BackgroundEffect>,
}

impl Blur {
	pub fn new(window: Arc<Window>) -> Self {
		#[cfg(all(unix, not(target_os = "macos")))]
		let effect = wayland::BackgroundEffect::new(&window);
		Self {
			#[cfg(all(unix, not(target_os = "macos")))]
			effect,
			window,
		}
	}

	/// Turns the blur behind the window on or off, from its next frame on
	pub fn set(&mut self, blurred: bool) {
		#[cfg(all(unix, not(target_os = "macos")))]
		if let Some(effect) = &mut self.effect {
			effect.set(blurred);
			return;
		}
		self.window.set_blur(blurred);
	}
}

#[cfg(all(unix, not(target_os = "macos")))]
mod wayland {

	use wayland_backend::client::{Backend, ObjectId};
	use wayland_client::globals::{GlobalListContents, registry_queue_init};
	use wayland_client::protocol::wl_compositor::WlCompositor;
	use wayland_client::protocol::wl_region::WlRegion;
	use wayland_client::protocol::wl_registry::WlRegistry;
	use wayland_client::protocol::wl_surface::WlSurface;
	use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
	use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
	use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
	use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
	use winit::window::Window;

	/// The compositor's background effects for one window
	pub(super) struct BackgroundEffect {
		connection: Connection,

		/// Where the protocol's events go; nothing reads them, and they're few
		_queue: EventQueue<State>,
		handle: QueueHandle<State>,
		manager: ExtBackgroundEffectManagerV1,
		compositor: WlCompositor,
		surface: WlSurface,

		/// The blur, while it's on
		effect: Option<ExtBackgroundEffectSurfaceV1>,
	}

	impl BackgroundEffect {
		/// The background effects for `window`, if it's a Wayland window and the compositor offers them. They use winit's
		/// own connection, so `window` must outlive them.
		pub(super) fn new(window: &Window) -> Option<Self> {
			let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else { return None };
			let RawWindowHandle::Wayland(surface) = window.window_handle().ok()?.as_raw() else { return None };

			// SAFETY: the display is winit's, which lives as long as the window, and so as long as this.
			let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
			let connection = Connection::from_backend(backend);
			let (globals, queue) = registry_queue_init::<State>(&connection).ok()?;
			let handle = queue.handle();
			let manager = globals.bind(&handle, 1..=1, ()).ok()?;
			let compositor = globals.bind(&handle, 1..=6, ()).ok()?;
			// SAFETY: the surface is the window's, alive as long as the window.
			let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }.ok()?;
			let surface = WlSurface::from_id(&connection, id).ok()?;
			Some(Self { connection, _queue: queue, handle, manager, compositor, surface, effect: None })
		}

		/// Turns the blur behind the whole window on or off, from the window's next frame on
		pub(super) fn set(&mut self, blurred: bool) {
			match (blurred, self.effect.take()) {
				(true, None) => {
					let effect = self.manager.get_background_effect(&self.surface, &self.handle, ());
					// Clipped by the compositor to the window, whatever its size
					let region = self.compositor.create_region(&self.handle, ());
					region.add(0, 0, i32::MAX, i32::MAX);
					effect.set_blur_region(Some(&region));
					region.destroy();
					self.effect = Some(effect);
				},
				(true, effect) => self.effect = effect,
				(false, Some(effect)) => effect.destroy(),
				(false, None) => (),
			}
			if let Err(error) = self.connection.flush() {
				tracing::warn!("Couldn't ask the compositor for blur: {error}");
			}
		}
	}

	impl Drop for BackgroundEffect {
		fn drop(&mut self) {
			self.set(false);
			self.manager.destroy();
		}
	}

	/// The protocol objects' events, which nothing needs
	struct State;

	impl Dispatch<WlRegistry, GlobalListContents> for State {
		fn event(
			_: &mut Self,
			_: &WlRegistry,
			_: <WlRegistry as Proxy>::Event,
			_: &GlobalListContents,
			_: &Connection,
			_: &QueueHandle<Self>,
		) {
		}
	}

	delegate_noop!(State: ignore ExtBackgroundEffectManagerV1);
	delegate_noop!(State: ignore ExtBackgroundEffectSurfaceV1);
	delegate_noop!(State: WlCompositor);
	delegate_noop!(State: WlRegion);
}
