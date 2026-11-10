//! How nvui's settings (`rpc::nvui::Settings`, set with `require("nvui").setup()`) apply to the renderer

use std::time::Duration;

use renderer::{Easing, Motion, UnderlineSize};
use rpc::nvui::{Animation, Settings};

/// How the renderer moves scrolled text
pub fn scroll_motion(settings: &Settings) -> Motion {
	let ease = |easing, duration: f32| Motion::Ease { easing, duration: Duration::from_secs_f32(duration / 1000.0) };
	match settings.scroll.animation {
		// Over no time at all, the text is in place at once.
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
