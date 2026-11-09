//! How animated values move: along an easing curve over a fixed time, or like a spring.

use std::time::{Duration, Instant};

/// How an animated value moves to its target
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
	/// Along `easing`, taking `duration`. A new target restarts the curve from where the value is.
	Ease { easing: Easing, duration: Duration },

	/// Like a weight on a spring (of mass 1), pulled by `stiffness` and slowed by `damping`. With less damping than
	/// `2 * stiffness.sqrt()`, it overshoots and bounces; with more, it creeps in. A new target keeps the value's
	/// speed, so motion stays smooth when targets change quickly, e.g. while scrolling continuously.
	Spring { stiffness: f32, damping: f32 },
}

/// The shape of a [`Motion::Ease`]: how far along the way the value is at each point in time
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
	/// At constant speed
	Linear,

	/// Fast at first, slowing down at the end (cubic)
	EaseOut,

	/// Speeding up, then slowing down (cubic)
	EaseInOut,

	/// A cubic Bézier curve from (0, 0) to (1, 1) with control points (x1, y1) and (x2, y2), like CSS's
	/// `cubic-bezier()`: x is time, y the progress. A y outside 0–1 overshoots.
	CubicBezier(f32, f32, f32, f32),
}

impl Easing {
	/// Progress at `time`, both from 0 to 1
	fn progress(self, time: f32) -> f32 {
		match self {
			Easing::Linear => time,
			Easing::EaseOut => 1.0 - (1.0 - time).powi(3),
			Easing::EaseInOut if time < 0.5 => 4.0 * time.powi(3),
			Easing::EaseInOut => 1.0 - (2.0 - 2.0 * time).powi(3) / 2.0,
			Easing::CubicBezier(x1, y1, x2, y2) => {
				let t = bezier_parameter(time, x1, x2);
				bezier(t, y1, y2)
			},
		}
	}
}

/// One coordinate of a cubic Bézier curve from 0 to 1 with control points `p1` and `p2`, at parameter `t`
fn bezier(t: f32, p1: f32, p2: f32) -> f32 {
	let u = 1.0 - t;
	3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

/// The parameter at which a Bézier curve with x control points `x1` and `x2` reaches `x`. x grows along the curve
/// for control points within 0–1, as CSS requires, so bisection finds it.
fn bezier_parameter(x: f32, x1: f32, x2: f32) -> f32 {
	let (x1, x2) = (x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0));
	let (mut low, mut high) = (0.0, 1.0);
	// 20 halvings: within a millionth, far below a pixel
	for _ in 0..20 {
		let middle = (low + high) / 2.0;
		if bezier(middle, x1, x2) < x {
			low = middle;
		} else {
			high = middle;
		}
	}
	(low + high) / 2.0
}

/// Distance and speed below which a spring counts as at rest: a hundredth of a row is a fraction of a pixel.
const REST_DISTANCE: f32 = 0.01;
const REST_SPEED: f32 = 0.1;

/// A value moving towards 0
#[derive(Debug, Clone, Copy)]
pub(crate) struct Approach {
	motion: Motion,

	/// The value and its speed (units per second) when it started moving
	from: f32,
	speed: f32,
	started: Instant,
}

impl Approach {
	/// A value at rest at 0
	pub(crate) fn new(motion: Motion, now: Instant) -> Self {
		Self { motion, from: 0.0, speed: 0.0, started: now }
	}

	/// The value at `now`; exactly 0 once it got there
	pub(crate) fn value(&self, now: Instant) -> f32 {
		let elapsed = now.saturating_duration_since(self.started).as_secs_f32();
		match self.motion {
			Motion::Ease { easing, duration } => {
				let time = elapsed / duration.as_secs_f32().max(f32::EPSILON);
				if time >= 1.0 { 0.0 } else { self.from * (1.0 - easing.progress(time)) }
			},
			Motion::Spring { .. } => {
				let (value, speed) = self.spring(elapsed);
				if value.abs() < REST_DISTANCE && speed.abs() < REST_SPEED { 0.0 } else { value }
			},
		}
	}

	/// Adds `amount` to the value at `now`, and moves on from there. A spring keeps its speed; an easing curve starts
	/// over.
	pub(crate) fn add(&mut self, amount: f32, now: Instant) {
		let elapsed = now.saturating_duration_since(self.started).as_secs_f32();
		let (value, speed) = match self.motion {
			Motion::Ease { .. } => (self.value(now), 0.0),
			Motion::Spring { .. } => self.spring(elapsed),
		};
		(self.from, self.speed, self.started) = (value + amount, speed, now);
	}

	/// The value and speed of a spring `elapsed` seconds after it started: the solution of x'' = -k·x - c·x'
	fn spring(&self, elapsed: f32) -> (f32, f32) {
		let Motion::Spring { stiffness, damping } = self.motion else { return (0.0, 0.0) };
		let (x0, v0, t) = (self.from, self.speed, elapsed);
		let omega = stiffness.max(f32::EPSILON).sqrt();
		let zeta = damping / (2.0 * omega);

		if zeta < 1.0 {
			// Underdamped: an oscillation dying out
			let omega_d = omega * (1.0 - zeta * zeta).sqrt();
			let (a, b) = (x0, (v0 + zeta * omega * x0) / omega_d);
			let (sin, cos) = (omega_d * t).sin_cos();
			let decay = (-zeta * omega * t).exp();
			let value = decay * (a * cos + b * sin);
			let speed = decay * ((b * omega_d - zeta * omega * a) * cos - (a * omega_d + zeta * omega * b) * sin);
			(value, speed)
		} else if zeta == 1.0 {
			// Critically damped: the fastest approach without overshooting
			let b = v0 + omega * x0;
			let decay = (-omega * t).exp();
			(decay * (x0 + b * t), decay * (b - omega * (x0 + b * t)))
		} else {
			// Overdamped: two decaying terms
			let root = omega * (zeta * zeta - 1.0).sqrt();
			let (r1, r2) = (-zeta * omega + root, -zeta * omega - root);
			let c1 = (v0 - r2 * x0) / (r1 - r2);
			let c2 = x0 - c1;
			let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
			(c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
		}
	}
}

#[cfg(test)]
mod tests {
	use std::time::{Duration, Instant};

	use super::{Approach, Easing, Motion};

	fn close(a: f32, b: f32) -> bool {
		(a - b).abs() < 1e-3
	}

	#[test]
	fn easings_run_from_0_to_1() {
		let easings = [Easing::Linear, Easing::EaseOut, Easing::EaseInOut, Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)];
		for easing in easings {
			assert!(close(easing.progress(0.0), 0.0), "{easing:?}");
			assert!(close(easing.progress(1.0), 1.0), "{easing:?}");
		}
		assert!(close(Easing::Linear.progress(0.25), 0.25));
		assert!(close(Easing::EaseOut.progress(0.5), 0.875));
		assert!(close(Easing::EaseInOut.progress(0.5), 0.5));
	}

	#[test]
	fn cubic_bezier_matches_known_curves() {
		// Control points on the diagonal make a straight line.
		assert!(close(Easing::CubicBezier(0.25, 0.25, 0.75, 0.75).progress(0.3), 0.3));
		// CSS's ease-in-out, (0.42, 0, 0.58, 1), is symmetric around the middle.
		let css = Easing::CubicBezier(0.42, 0.0, 0.58, 1.0);
		assert!(close(css.progress(0.5), 0.5));
		assert!(close(css.progress(0.2) + css.progress(0.8), 1.0));
		// A y beyond 1 overshoots.
		assert!(Easing::CubicBezier(0.3, 1.6, 0.6, 1.0).progress(0.6) > 1.0);
	}

	#[test]
	fn eased_value_reaches_0_after_its_duration() {
		let now = Instant::now();
		let motion = Motion::Ease { easing: Easing::Linear, duration: Duration::from_millis(100) };
		let mut value = Approach::new(motion, now);
		value.add(4.0, now);
		assert!(close(value.value(now), 4.0));
		assert!(close(value.value(now + Duration::from_millis(25)), 3.0));
		assert_eq!(value.value(now + Duration::from_millis(100)), 0.0);

		// Adding while moving starts over from where it is.
		value.add(2.0, now + Duration::from_millis(50));
		assert!(close(value.value(now + Duration::from_millis(50)), 4.0));
	}

	#[test]
	fn springs_settle_at_0() {
		let now = Instant::now();
		// Under-, critically and overdamped
		for damping in [10.0, 40.0, 80.0] {
			let mut value = Approach::new(Motion::Spring { stiffness: 400.0, damping }, now);
			value.add(10.0, now);
			assert!(close(value.value(now), 10.0), "{damping}");
			assert!(value.value(now + Duration::from_millis(50)).abs() < 10.0, "{damping}");
			assert_eq!(value.value(now + Duration::from_secs(5)), 0.0, "{damping}");
		}
	}

	#[test]
	fn an_underdamped_spring_overshoots() {
		let now = Instant::now();
		let mut value = Approach::new(Motion::Spring { stiffness: 400.0, damping: 10.0 }, now);
		value.add(10.0, now);
		let lowest = (0..500)
			.map(|ms| value.value(now + Duration::from_millis(ms)))
			.fold(f32::MAX, f32::min);
		assert!(lowest < -1.0, "{lowest}");
	}

	#[test]
	fn a_spring_keeps_its_speed_when_added_to() {
		let now = Instant::now();
		let mut value = Approach::new(Motion::Spring { stiffness: 400.0, damping: 40.0 }, now);
		value.add(10.0, now);
		let later = now + Duration::from_millis(30);
		let (before, speed) = value.spring(0.03);
		value.add(5.0, later);
		assert!(close(value.value(later), before + 5.0));
		assert!(close(value.speed, speed));
		assert!(speed < 0.0);
	}
}
