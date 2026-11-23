//! Cursor animation: the cursor's corners move to its new place each on its own, the leading ones faster, so the
//! cursor stretches towards where it goes and shrinks back as it arrives.

use std::time::{Duration, Instant};

use crate::animation::{Approach, Easing, Motion};
use crate::{BarOrientation, Cursor, CursorShape, Layer, Rect};

/// How the cursor makes a move to a new place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorMotion {
	/// How its corners move there.
	pub motion: Motion,

	/// How much the trailing corners lag behind the leading ones: from 0, where the cursor moves as it is, to 1, where
	/// the leading corners are there at once.
	pub trail: f32,
}

/// Below this share of a motion's time, a corner is at its place at once.
const INSTANT: f32 = 0.01;

/// The motion of a corner at rest: any, as it has no way to go.
const AT_REST: Motion = Motion::Ease { easing: Easing::Linear, duration: Duration::ZERO };

/// The cursor's corners on their way to where the cursor is.
#[derive(Default)]
pub(crate) struct CursorAnimation {
	/// Top-left, top-right, bottom-right and bottom-left; none before the first cursor.
	corners: Option<[Corner; 4]>,
}

#[derive(Debug, Clone, Copy)]
struct Corner {
	/// Where the corner goes, in pixels from the window's top-left corner: x, y. In pixels, not cells, so the cursor can
	/// move between layers with cells of different sizes.
	target: [f32; 2],

	/// How far it still is from there, in pixels, each moving towards 0.
	offset: [Approach; 2],
}

impl CursorAnimation {
	/// Moves the cursor to `rect`, in pixels, at `now`: as `motion` says, or at once without one or for the first
	/// cursor.
	pub(crate) fn update(&mut self, rect: Rect<f32>, motion: Option<CursorMotion>, now: Instant) {
		let targets = corners_of(rect);
		let at_rest = |target: [f32; 2]| Corner { target, offset: [Approach::new(AT_REST, now); 2] };
		let Some(corners) = &mut self.corners else {
			self.corners = Some(targets.map(at_rest));
			return;
		};
		if corners.iter().zip(&targets).all(|(corner, target)| corner.target == *target) {
			return;
		}
		let Some(CursorMotion { motion, trail }) = motion else {
			*corners = targets.map(at_rest);
			return;
		};

		let (from, to) = (center(&corners.map(|corner| corner.target)), center(&targets));
		let travel = [to[0] - from[0], to[1] - from[1]];
		for ((corner, target), share) in corners.iter_mut().zip(targets).zip(lead(travel, trail)) {
			for ((offset, from), to) in corner.offset.iter_mut().zip(corner.target).zip(target) {
				if share < INSTANT {
					*offset = Approach::new(motion, now);
				} else {
					offset.add_with(from - to, motion.scaled(share), now);
				}
			}
			corner.target = target;
		}
	}

	/// Where the corners are at `now`, in pixels: top-left, top-right, bottom-right and bottom-left.
	pub(crate) fn corners(&self, now: Instant) -> Option<[[f32; 2]; 4]> {
		let corners = self.corners.as_ref()?;
		Some(corners.map(|corner| [0, 1].map(|axis| corner.target[axis] + corner.offset[axis].value(now))))
	}

	/// Whether the cursor is still on its way at `now`.
	pub(crate) fn is_animating(&self, now: Instant) -> bool {
		let corners = self.corners.iter().flatten();
		corners.flat_map(|corner| &corner.offset).any(|offset| offset.value(now) != 0.0)
	}
}

/// Where `cursor` of `layer` is, in pixels from the window's top-left corner. A bar is as
/// thick as it's drawn at rest.
pub(crate) fn cursor_rect(layer: &Layer, cursor: Cursor) -> Rect<f32> {
	let cell = layer.cell;
	let (left, top) = (layer.x + cursor.col * cell.width, layer.y + cursor.row * cell.height);
	let width = cursor.width * cell.width;
	// Like `cursor_quad`.
	let thickness = |size: u32, percentage: u32| (size * percentage).div_ceil(100).clamp(1, size);
	let rect = match cursor.shape {
		CursorShape::Block | CursorShape::Outline => Rect::new(left, top, width, cell.height),
		CursorShape::Bar { orientation: BarOrientation::Vertical, percentage } => {
			Rect::new(left, top, thickness(cell.width, percentage), cell.height)
		},
		CursorShape::Bar { orientation: BarOrientation::Horizontal, percentage } => {
			let height = thickness(cell.height, percentage);
			Rect::new(left, top + cell.height - height, width, height)
		},
	};
	rect.to_f32()
}

/// The corners of `rect`: top-left, top-right, bottom-right and bottom-left.
fn corners_of(rect: Rect<f32>) -> [[f32; 2]; 4] {
	let (left, top, right, bottom) = (rect.x, rect.y, rect.right(), rect.bottom());
	[[left, top], [right, top], [right, bottom], [left, bottom]]
}

fn center(corners: &[[f32; 2]; 4]) -> [f32; 2] {
	[0, 1].map(|axis| corners.iter().map(|corner| corner[axis]).sum::<f32>() / 4.0)
}

/// How long each corner takes, as a share of the motion's time, on a move of `travel` pixels: 1 for the corner most
/// behind, `1 - trail` for the one most ahead, in between for the others by how much they face the way.
fn lead(travel: [f32; 2], trail: f32) -> [f32; 4] {
	// Each corner's direction from the cursor's center.
	const DIRECTIONS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
	let ahead = DIRECTIONS.map(|[x, y]| x * travel[0] + y * travel[1]);
	let (least, most) = ahead
		.iter()
		.fold((f32::MAX, f32::MIN), |(least, most), &a| (least.min(a), most.max(a)));
	// Without a way to go, e.g. when only the shape changes, every corner takes the whole time.
	if most <= least {
		return [1.0; 4];
	}
	ahead.map(|ahead| 1.0 - trail.clamp(0.0, 1.0) * (ahead - least) / (most - least))
}

#[cfg(test)]
mod tests {
	use std::time::{Duration, Instant};

	use super::{CursorAnimation, CursorMotion, cursor_rect, lead};
	use crate::{BarOrientation, CellSize, Cursor, CursorShape, Easing, Layer, Motion};

	const CELL: CellSize = CellSize { width: 10, height: 20 };

	fn linear(millis: u64) -> Motion {
		Motion::Ease { easing: Easing::Linear, duration: Duration::from_millis(millis) }
	}

	fn motion(trail: f32) -> Option<CursorMotion> {
		Some(CursorMotion { motion: linear(100), trail })
	}

	/// A block cursor of a one-pixel cell.
	fn block(col: f32, row: f32) -> crate::Rect<f32> {
		crate::Rect::new(col, row, 1.0, 1.0)
	}

	fn close(a: [[f32; 2]; 4], b: [[f32; 2]; 4]) -> bool {
		a.iter().flatten().zip(b.iter().flatten()).all(|(a, b)| (a - b).abs() < 1e-3)
	}

	#[test]
	fn leading_corners_move_faster() {
		// Right: both right corners lead, both left ones trail.
		assert_eq!(lead([10.0, 0.0], 0.5), [1.0, 0.5, 0.5, 1.0]);
		// Down and right: the bottom-right corner leads, the top-left one trails, the others are in between.
		assert_eq!(lead([10.0, 10.0], 0.5), [1.0, 0.75, 0.5, 0.75]);
		// No trail: a rigid move.
		assert_eq!(lead([-10.0, 3.0], 0.0), [1.0; 4]);
		// Only the shape changes.
		assert_eq!(lead([0.0, 0.0], 0.5), [1.0; 4]);
	}

	#[test]
	fn the_first_cursor_is_in_place_at_once() {
		let (now, mut cursor, moving) = (Instant::now(), CursorAnimation::default(), motion(0.5));
		assert_eq!(cursor.corners(now), None);
		cursor.update(block(3.0, 2.0), moving, now);
		assert_eq!(cursor.corners(now), Some([[3.0, 2.0], [4.0, 2.0], [4.0, 3.0], [3.0, 3.0]]));
		assert!(!cursor.is_animating(now));
	}

	#[test]
	fn a_move_stretches_the_cursor_then_settles() {
		let (now, mut cursor, moving) = (Instant::now(), CursorAnimation::default(), motion(0.5));
		cursor.update(block(0.0, 0.0), moving, now);
		cursor.update(block(10.0, 0.0), moving, now);
		assert!(cursor.is_animating(now));

		// Halfway through the leading corners' time: they're halfway, the trailing ones a quarter of the way.
		let corners = cursor.corners(now + Duration::from_millis(25)).unwrap();
		assert!(close(corners, [[2.5, 0.0], [6.0, 0.0], [6.0, 1.0], [2.5, 1.0]]), "{corners:?}");
		// The leading corners are there, the trailing ones still on their way: the cursor is at its longest.
		let corners = cursor.corners(now + Duration::from_millis(50)).unwrap();
		assert!(close(corners, [[5.0, 0.0], [11.0, 0.0], [11.0, 1.0], [5.0, 1.0]]), "{corners:?}");

		let later = now + Duration::from_millis(100);
		assert_eq!(cursor.corners(later), Some([[10.0, 0.0], [11.0, 0.0], [11.0, 1.0], [10.0, 1.0]]));
		assert!(!cursor.is_animating(later));
	}

	#[test]
	fn a_move_without_animation_jumps() {
		let (now, mut cursor, moving) = (Instant::now(), CursorAnimation::default(), motion(0.5));
		cursor.update(block(0.0, 0.0), moving, now);
		cursor.update(block(10.0, 5.0), None, now);
		assert!(!cursor.is_animating(now));
		assert_eq!(cursor.corners(now).unwrap()[0], [10.0, 5.0]);
	}

	#[test]
	fn a_new_target_on_the_way_starts_from_where_the_corners_are() {
		let (now, mut cursor, moving) = (Instant::now(), CursorAnimation::default(), motion(0.0));
		cursor.update(block(0.0, 0.0), moving, now);
		cursor.update(block(10.0, 0.0), moving, now);
		let halfway = now + Duration::from_millis(50);
		cursor.update(block(10.0, 4.0), moving, halfway);
		// No jump: still halfway along the row, and only starting down.
		let corners = cursor.corners(halfway).unwrap();
		assert!(close(corners, [[5.0, 0.0], [6.0, 0.0], [6.0, 1.0], [5.0, 1.0]]), "{corners:?}");
	}

	#[test]
	fn a_shape_change_animates_in_place() {
		let (now, mut cursor, moving) = (Instant::now(), CursorAnimation::default(), motion(0.5));
		cursor.update(block(2.0, 0.0), moving, now);
		cursor.update(crate::Rect::new(2.0, 0.0, 0.2, 1.0), moving, now);
		// Halfway through the motion, the right edge is on its way from 3 to 2.2, the left one in place.
		let corners = cursor.corners(now + Duration::from_millis(50)).unwrap();
		assert!(close(corners, [[2.0, 0.0], [2.6, 0.0], [2.6, 1.0], [2.0, 1.0]]), "{corners:?}");
	}

	#[test]
	fn a_held_key_keeps_the_cursor_going_one_way() {
		// `j` held down: a row every 30 ms, faster than the spring settles.
		let spring = Some(CursorMotion { motion: Motion::Spring { stiffness: 700.0, damping: 53.0 }, trail: 1.0 });
		let (start, mut cursor) = (Instant::now(), CursorAnimation::default());
		let mut last = f32::NEG_INFINITY;
		for step in 0..50 {
			let now = start + Duration::from_millis(30 * step);
			cursor.update(block(0.0, step as f32 * 20.0), spring, now);
			// Drawn every 5 ms in between: the top edge only ever goes down.
			for frame in 0..6 {
				let top = cursor.corners(now + Duration::from_millis(5 * frame)).unwrap()[0][1];
				assert!(top >= last - 1e-3, "back up from {last} to {top} at move {step}");
				last = top;
			}
		}
	}

	#[test]
	fn cursor_rects() {
		// At row 1, column 2 of the window's cells.
		let layer = Layer {
			id: 1,
			x: 20,
			y: 20,
			cell: CELL,
			font: crate::FontId::MAIN,
			width: 10,
			height: 5,
			cells: Vec::new(),
			cursor: None,
			scroll: None,
			shadow: None,
			outline: None,
			padding: crate::Edges::default(),
			padding_background: (0, 0),
			corner_radius: 0.0,
		};
		let cursor = |shape| Cursor { row: 2, col: 3, width: 1, shape, color: 0, text_color: 0, motion: None };
		assert_eq!(cursor_rect(&layer, cursor(CursorShape::Block)), crate::Rect::new(50.0, 60.0, 10.0, 20.0));
		let bar = |orientation, percentage| CursorShape::Bar { orientation, percentage };
		// 25% of 10 pixels rounds up to 3, as drawn at rest.
		assert_eq!(
			cursor_rect(&layer, cursor(bar(BarOrientation::Vertical, 25))),
			crate::Rect::new(50.0, 60.0, 3.0, 20.0)
		);
		assert_eq!(
			cursor_rect(&layer, cursor(bar(BarOrientation::Horizontal, 20))),
			crate::Rect::new(50.0, 76.0, 10.0, 4.0)
		);
	}
}
