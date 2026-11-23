//! Sarthi's animation settings as the renderer moves things

use std::time::Duration;

use renderer::{Easing, Motion};
use rpc::sarthi::Animation;

/// `animation` as the renderer takes it; none for `none`, which jumps
pub fn motion(animation: Animation) -> Option<Motion> {
	let ease = |easing, duration: f32| Motion::Ease { easing, duration: Duration::from_secs_f32(duration / 1000.0) };
	Some(match animation {
		Animation::None => return None,
		Animation::Spring { stiffness, damping } => Motion::Spring { stiffness, damping },
		Animation::Linear { duration } => ease(Easing::Linear, duration),
		Animation::EaseOut { duration } => ease(Easing::EaseOut, duration),
		Animation::EaseInOut { duration } => ease(Easing::EaseInOut, duration),
		Animation::CubicBezier { points: [x1, y1, x2, y2], duration } => {
			ease(Easing::CubicBezier(x1, y1, x2, y2), duration)
		},
	})
}
