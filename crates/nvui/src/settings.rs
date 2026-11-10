//! How nvui's settings (`rpc::nvui::Settings`, set with `require("nvui").setup()`) apply to the renderer

use std::time::Duration;

use renderer::{CursorMotion, Easing, Motion, UnderlineSize};
use rpc::nvui::{Animation, ScrolledWindow, Settings};

/// How the renderer moves scrolled text
pub fn scroll_motion(settings: &Settings) -> Motion {
	motion(settings.scroll.animation)
}

/// How the renderer moves the cursor
pub fn cursor_motion(settings: &Settings) -> CursorMotion {
	let cursor = &settings.cursor;
	CursorMotion { motion: motion(cursor.animation), short: motion(cursor.short_animation), trail: cursor.trail }
}

/// `animation` as the renderer takes it
fn motion(animation: Animation) -> Motion {
	let ease = |easing, duration: f32| Motion::Ease { easing, duration: Duration::from_secs_f32(duration / 1000.0) };
	match animation {
		// Over no time at all, the value is in place at once.
		Animation::None => ease(Easing::Linear, 0.0),
		Animation::Spring { stiffness, damping } => Motion::Spring { stiffness, damping },
		Animation::Linear { duration } => ease(Easing::Linear, duration),
		Animation::EaseOut { duration } => ease(Easing::EaseOut, duration),
		Animation::EaseInOut { duration } => ease(Easing::EaseInOut, duration),
		Animation::CubicBezier { points: [x1, y1, x2, y2], duration } => {
			ease(Easing::CubicBezier(x1, y1, x2, y2), duration)
		},
	}
}

/// Whether `window`'s text slides when it scrolls: unless its filetype is excluded, or its `w:nvui_smooth_scroll` says
/// otherwise
pub fn smooth_scroll(settings: &Settings, window: &ScrolledWindow) -> bool {
	let excluded = settings.scroll.exclude_filetypes.contains(&window.filetype);
	window.smooth_scroll.unwrap_or(!excluded)
}

/// How the renderer draws underlines, in logical pixels
pub fn underlines(settings: &Settings) -> UnderlineSize {
	let underline = &settings.underline;
	UnderlineSize {
		offset: underline.offset,
		thickness: underline.thickness,
		wave_height: underline.wave_height,
		ink_gaps: underline.ink_gaps,
	}
}

#[cfg(test)]
mod tests {
	use rpc::neovim::Window;
	use rpc::nvui::{ScrolledWindow, Settings};

	use super::smooth_scroll;

	#[test]
	fn which_windows_slide() {
		let mut settings = Settings::default();
		settings.scroll.exclude_filetypes = vec![String::from("cmp_menu")];
		let window = |filetype: &str, smooth_scroll| ScrolledWindow {
			win: Window(1000),
			filetype: filetype.into(),
			smooth_scroll,
		};

		assert!(smooth_scroll(&settings, &window("lua", None)));
		assert!(!smooth_scroll(&settings, &window("cmp_menu", None)));
		// The window's variable wins.
		assert!(smooth_scroll(&settings, &window("cmp_menu", Some(true))));
		assert!(!smooth_scroll(&settings, &window("lua", Some(false))));
		// Nothing is excluded by default.
		assert!(smooth_scroll(&Settings::default(), &window("cmp_menu", None)));
	}
}
