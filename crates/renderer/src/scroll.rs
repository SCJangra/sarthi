//! Smooth scrolling: text that scrolled slides to its new place instead of jumping there.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::time::Instant;

use crate::animation::{Approach, Motion};
use crate::{CellLayer, Decoration, Layer, RenderCell, Scroll};

/// The layers whose text is sliding, by layer id
pub(crate) struct ScrollAnimations {
	/// How scrolled text moves to its new place
	motion: Motion,

	layers: HashMap<u64, Animation>,
}

/// The scrolling region of a layer while its text slides
pub(crate) struct SlidingRegion {
	/// The region's text, from the first row that shows: the layer's text, with text from before the scroll above or
	/// below it
	pub layer: CellLayer,

	/// Rows below the region's top to draw `layer`, or above if negative
	pub offset: f32,

	/// The region's height in rows. What's drawn of `layer` outside it is cut off.
	pub height: u32,
}

/// Text of a layer sliding to its place
struct Animation {
	/// The layer's size and scrolling region; the animation ends if they change
	size: (u32, u32),
	region: Scroll,

	/// Rows the text is drawn below its place, or above if negative, moving towards 0
	offset: Approach,

	/// Text from before the scroll that left the region but is still drawn while sliding out, by row relative to the
	/// region's top: above the region after scrolling up, below it after scrolling down. Rows of the region's width.
	rows: BTreeMap<i32, Vec<RenderCell>>,
}

impl ScrollAnimations {
	pub(crate) fn new(motion: Motion) -> Self {
		Self { motion, layers: HashMap::new() }
	}

	/// Sets how scrolled text moves from the next scroll on
	pub(crate) fn set_motion(&mut self, motion: Motion) {
		self.motion = motion;
	}

	/// Starts or continues an animation for each of `layers` that scrolled since `shown`, the layers shown until now,
	/// and ends the animations that are done or whose layer is gone or changed size.
	pub(crate) fn update(&mut self, shown: &[Layer], layers: &[Layer], now: Instant) {
		self.layers.retain(|id, animation| {
			animation.offset(now) != 0.0
				&& layers
					.iter()
					.any(|Layer::Cells(layer)| layer.id == *id && animation.fits(layer, layer.scroll.as_ref()))
		});

		for Layer::Cells(layer) in layers {
			let Some(scroll) = layer.scroll else { continue };
			// Sliding text is drawn over the layer's own: through transparent backgrounds, both would show.
			if layer.cells.iter().any(|cell| cell.blend > 0) {
				continue;
			}
			// A new layer has no text to slide from; neither has one that changed size, e.g. a resized window.
			let previous = shown
				.iter()
				.find_map(|Layer::Cells(previous)| (previous.id == layer.id).then_some(previous));
			let Some(previous) =
				previous.filter(|previous| (previous.width, previous.height) == (layer.width, layer.height))
			else {
				continue;
			};
			let fits = scroll.bottom <= layer.height && scroll.right <= layer.width;
			if !fits || scroll.top >= scroll.bottom || scroll.left >= scroll.right {
				continue;
			}

			let animation = self.layers.entry(layer.id).or_insert_with(|| Animation {
				size: (layer.width, layer.height),
				region: scroll,
				offset: Approach::new(self.motion, now),
				rows: BTreeMap::new(),
			});
			animation.scroll(previous, scroll, now);
		}
	}

	/// Whether any text is still sliding
	pub(crate) fn is_animating(&self, now: Instant) -> bool {
		self.layers.values().any(|animation| animation.offset(now) != 0.0)
	}

	/// The scrolling region of `layer` while its text slides, or `None` if nothing slides
	pub(crate) fn region(&self, layer: &CellLayer, now: Instant) -> Option<SlidingRegion> {
		let animation = self.layers.get(&layer.id)?;
		let offset = animation.offset(now);
		(offset != 0.0).then(|| animation.region_layer(layer, offset))
	}
}

impl Animation {
	/// Whether this animation still fits `layer`, which scrolled within `scroll` if it did
	fn fits(&self, layer: &CellLayer, scroll: Option<&Scroll>) -> bool {
		let region = |scroll: &Scroll| (scroll.top, scroll.bottom, scroll.left, scroll.right);
		self.size == (layer.width, layer.height) && scroll.is_none_or(|scroll| region(scroll) == region(&self.region))
	}

	/// Rows the text is drawn below its place at `now`, or above if negative
	fn offset(&self, now: Instant) -> f32 {
		self.offset.value(now)
	}

	fn height(&self) -> i32 {
		(self.region.bottom - self.region.top) as i32
	}

	/// Moves the text by `scroll.rows`, from where it's drawn at `now`, and moves on from there. The text slides at
	/// most the region's height: after a longer jump, the text of `previous` (the layer as shown until now) slides out
	/// and the new text slides in right behind it.
	fn scroll(&mut self, previous: &CellLayer, scroll: Scroll, now: Instant) {
		let (offset, height) = (self.offset(now), self.height());
		let target = (offset + scroll.rows as f32).clamp(-height as f32, height as f32);
		let rows = (target - offset).round() as i32;

		let region_rows = region_rows(previous, &self.region).map(Vec::from);
		let mut moved: BTreeMap<i32, Vec<RenderCell>> = std::mem::take(&mut self.rows)
			.into_iter()
			.chain((0..).zip(region_rows))
			.map(|(row, cells)| (row - rows, cells))
			.collect();

		self.offset.add(rows as f32, now);
		let hidden = self.hidden_rows(offset + rows as f32);
		moved.retain(|row, _| hidden.contains(row));
		self.rows = moved;
	}

	/// The rows outside the region that show while the text is drawn `offset` rows below its place, relative to the
	/// region's top
	fn hidden_rows(&self, offset: f32) -> Range<i32> {
		let rows = offset.abs().ceil() as i32;
		if offset > 0.0 { -rows..0 } else { self.height()..self.height() + rows }
	}

	/// The region of `layer` while its text is drawn `offset` rows below its place
	fn region_layer(&self, layer: &CellLayer, offset: f32) -> SlidingRegion {
		let region = &self.region;
		let hidden = self.hidden_rows(offset);
		let first = hidden.start.min(0);
		let rows = first..hidden.end.max(self.height());

		let new_rows: Vec<&[RenderCell]> = region_rows(layer, region).collect();
		// A curve that overshoots shows rows past the text from before the scroll, or past the new text, which nvui
		// doesn't have: they're blank, in the background of the row at that edge.
		let (above, below) = (blank(new_rows[0]), blank(new_rows[new_rows.len() - 1]));
		let cells: Vec<&[RenderCell]> = rows
			.map(|row| match self.rows.get(&row) {
				_ if (0..self.height()).contains(&row) => new_rows[row as usize],
				Some(cells) => cells,
				None if row < 0 => &above,
				None => &below,
			})
			.collect();
		let cells = cells.concat();

		let width = region.right - region.left;
		// The cursor doesn't slide: it's drawn where Neovim put it, over the region.
		let region_layer = CellLayer {
			id: layer.id,
			row: layer.row + region.top,
			col: layer.col + region.left,
			width,
			height: cells.len() as u32 / width,
			cells,
			cursor: None,
			scroll: None,
			shadow: None,
		};
		SlidingRegion { layer: region_layer, offset: first as f32 + offset, height: self.height() as u32 }
	}
}

/// A row like `cells`, with nothing but their backgrounds
fn blank(cells: &[RenderCell]) -> Vec<RenderCell> {
	let space = arrayvec::ArrayString::from(" ").expect("a space fits in a cell");
	cells
		.iter()
		.map(|cell| RenderCell { text: space, decoration: Decoration::default(), ..*cell })
		.collect()
}

/// The rows of `layer` in the scrolling region of `scroll`
fn region_rows<'a>(layer: &'a CellLayer, scroll: &Scroll) -> impl Iterator<Item = &'a [RenderCell]> {
	let width = layer.width as usize;
	let (left, right) = (scroll.left as usize, scroll.right as usize);
	layer
		.cells
		.chunks_exact(width.max(1))
		.take(scroll.bottom as usize)
		.skip(scroll.top as usize)
		.map(move |row| &row[left..right])
}

#[cfg(test)]
mod tests {
	use std::time::{Duration, Instant};

	use arrayvec::ArrayString;

	use super::ScrollAnimations;
	use crate::animation::{Easing, Motion};
	use crate::{BarOrientation, CellLayer, Cursor, CursorShape, Decoration, Layer, RenderCell, Scroll, TextStyle};

	const DURATION: Duration = Duration::from_millis(150);

	/// Animations that ease out over [`DURATION`]
	fn animations() -> ScrollAnimations {
		ScrollAnimations::new(Motion::Ease { easing: Easing::EaseOut, duration: DURATION })
	}

	/// A layer one character per cell and one row per string, scrolled by `rows` within rows `top..bottom` if not 0
	fn layer(rows: &[&str], scroll: i32, top: u32, bottom: u32) -> Layer {
		let style = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
		let cells = rows
			.iter()
			.flat_map(|row| row.chars())
			.map(|c| RenderCell {
				text: ArrayString::from(&c.to_string()).unwrap(),
				style,
				background: 0,
				blend: 0,
				decoration: Decoration::default(),
			})
			.collect();
		let width = rows[0].len() as u32;
		let scroll = (scroll != 0).then_some(Scroll { rows: scroll, top, bottom, left: 0, right: width });
		Layer::Cells(CellLayer {
			id: 1,
			row: 0,
			col: 0,
			width,
			height: rows.len() as u32,
			cells,
			cursor: None,
			scroll,
			shadow: None,
		})
	}

	/// The rows of the sliding region of `layer` at `now`, and the rows it's drawn below the region's top
	fn region(scrolls: &ScrollAnimations, layer: &Layer, now: Instant) -> Option<(Vec<String>, f32)> {
		let Layer::Cells(layer) = layer;
		let region = scrolls.region(layer, now)?;
		let rows = region.layer.cells.chunks(region.layer.width as usize);
		Some((rows.map(|row| row.iter().map(|cell| cell.text.as_str()).collect()).collect(), region.offset))
	}

	#[test]
	fn scrolling_up_slides_the_text_up() {
		let (mut scrolls, now) = (animations(), Instant::now());
		let shown = [layer(&["a", "b", "c"], 0, 0, 3)];
		let scrolled = [layer(&["b", "c", "d"], 1, 0, 3)];
		scrolls.update(&shown, &scrolled, now);

		// At first, the row that scrolled out is drawn above the new text, which is a row lower: as shown before.
		assert_eq!(
			region(&scrolls, &scrolled[0], now),
			Some((vec!["a".into(), "b".into(), "c".into(), "d".into()], 0.0))
		);

		// Halfway, the text moved most of the way, and at the end it's in place.
		let (_, offset) = region(&scrolls, &scrolled[0], now + DURATION / 2).unwrap();
		assert!((-1.0..-0.5).contains(&offset), "{offset}");
		assert!(scrolls.is_animating(now + DURATION / 2));
		assert_eq!(region(&scrolls, &scrolled[0], now + DURATION), None);
		assert!(!scrolls.is_animating(now + DURATION));
	}

	#[test]
	fn transparent_text_doesnt_slide() {
		let (mut scrolls, now) = (animations(), Instant::now());
		let shown = [layer(&["a", "b", "c"], 0, 0, 3)];
		let [Layer::Cells(mut scrolled)] = [layer(&["b", "c", "d"], 1, 0, 3)];
		scrolled.cells[1].blend = 30;
		let scrolled = [Layer::Cells(scrolled)];
		scrolls.update(&shown, &scrolled, now);
		assert_eq!(region(&scrolls, &scrolled[0], now), None);
	}

	#[test]
	fn scrolling_down_slides_the_text_down() {
		let (mut scrolls, now) = (animations(), Instant::now());
		let shown = [layer(&["c", "d", "e"], 0, 0, 3)];
		let scrolled = [layer(&["a", "b", "c"], -2, 0, 3)];
		scrolls.update(&shown, &scrolled, now);

		// The new text starts two rows higher, with the rows that scrolled out below it.
		let rows = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
		assert_eq!(region(&scrolls, &scrolled[0], now), Some((rows, -2.0)));
	}

	#[test]
	fn a_jump_slides_one_region_height() {
		let (mut scrolls, now) = (animations(), Instant::now());
		let shown = [layer(&["a", "b", "c"], 0, 0, 3)];
		let jumped = [layer(&["x", "y", "z"], 100, 0, 3)];
		scrolls.update(&shown, &jumped, now);

		// The old text slides out and the new text follows right behind it.
		let rows = ["a", "b", "c", "x", "y", "z"].map(String::from).to_vec();
		assert_eq!(region(&scrolls, &jumped[0], now), Some((rows, 0.0)));
		// Halfway, 0.375 rows are left to go: only the last old row still shows.
		let rows = ["c", "x", "y", "z"].map(String::from).to_vec();
		assert_eq!(region(&scrolls, &jumped[0], now + DURATION / 2), Some((rows, -0.625)));
	}

	#[test]
	fn scrolls_add_up_while_sliding() {
		let (mut scrolls, now) = (animations(), Instant::now());
		let first = [layer(&["a", "b", "c"], 0, 0, 3)];
		let second = [layer(&["b", "c", "d"], 1, 0, 3)];
		let third = [layer(&["c", "d", "e"], 1, 0, 3)];
		scrolls.update(&first, &second, now);
		scrolls.update(&second, &third, now);

		let rows = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
		assert_eq!(region(&scrolls, &third[0], now), Some((rows, 0.0)));

		// Back up while sliding: the text that scrolled out above comes back.
		let back = [layer(&["b", "c", "d"], -1, 0, 3)];
		scrolls.update(&third, &back, now);
		let rows = ["a", "b", "c", "d"].map(String::from).to_vec();
		assert_eq!(region(&scrolls, &back[0], now), Some((rows, 0.0)));
	}

	#[test]
	fn rows_outside_the_region_stay() {
		let (mut scrolls, now) = (animations(), Instant::now());
		// A winbar above the text
		let shown = [layer(&["w", "a", "b", "c"], 0, 1, 4)];
		let mut scrolled = layer(&["w", "b", "c", "d"], 1, 1, 4);
		let Layer::Cells(cells) = &mut scrolled;
		let shape = CursorShape::Bar { orientation: BarOrientation::Vertical, percentage: 25 };
		cells.cursor = Some(Cursor { row: 2, col: 0, width: 1, shape, color: 0, text_color: 0, animate: true });
		scrolls.update(&shown, std::slice::from_ref(&scrolled), now);

		let Layer::Cells(cells) = &scrolled;
		let region = scrolls.region(cells, now).unwrap();
		assert_eq!((region.layer.row, region.layer.height, region.height), (1, 4, 3));
		// The cursor doesn't slide with the text: it's drawn where Neovim put it, over the region.
		assert_eq!(region.layer.cursor, None);
	}

	#[test]
	fn a_resize_ends_the_animation() {
		let (mut scrolls, now) = (animations(), Instant::now());
		let shown = [layer(&["a", "b", "c"], 0, 0, 3)];
		let scrolled = [layer(&["b", "c", "d"], 1, 0, 3)];
		scrolls.update(&shown, &scrolled, now);

		let resized = [layer(&["b", "c"], 0, 0, 2)];
		scrolls.update(&scrolled, &resized, now);
		assert!(!scrolls.is_animating(now));

		// Neither does a layer that scrolled as it was resized, or a new one, start one.
		scrolls.update(&resized, &scrolled, now);
		scrolls.update(&[], &[layer(&["a", "b"], 1, 0, 2)], now);
		assert!(!scrolls.is_animating(now));
	}

	#[test]
	fn overshooting_shows_blank_rows() {
		let now = Instant::now();
		// Like CSS's back-out: past the target, then back
		let motion = Motion::Ease { easing: Easing::CubicBezier(0.3, 1.6, 0.6, 1.0), duration: DURATION };
		let mut scrolls = ScrollAnimations::new(motion);
		let shown = [layer(&["a", "b", "c"], 0, 0, 3)];
		let scrolled = [layer(&["b", "c", "d"], 1, 0, 3)];
		scrolls.update(&shown, &scrolled, now);

		// Past its place, the text moved above the region's top, and a blank row follows it from below.
		let (rows, offset) = region(&scrolls, &scrolled[0], now + DURATION * 3 / 5).unwrap();
		assert_eq!(rows, ["b", "c", "d", " "]);
		assert!(offset < 0.0, "{offset}");
	}
}
