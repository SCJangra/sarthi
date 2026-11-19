//! Rectangles and the sizes around them, in pixels or cells

use std::ops::Add;

/// A rectangle: its top-left corner and its size, in pixels or cells
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect<T> {
	/// Left edge
	pub x: T,
	/// Top edge
	pub y: T,
	/// Width, from the left edge
	pub width: T,
	/// Height, from the top edge
	pub height: T,
}

impl<T: Copy + Add<Output = T>> Rect<T> {
	/// A rectangle at `x`, `y`, `width` × `height` large
	pub fn new(x: T, y: T, width: T, height: T) -> Self {
		Self { x, y, width, height }
	}

	/// Where it ends on the right, past its last column
	pub fn right(&self) -> T {
		self.x + self.width
	}

	/// Where it ends at the bottom, past its last row
	pub fn bottom(&self) -> T {
		self.y + self.height
	}
}

impl Rect<u32> {
	/// The rectangle grown by `edges` on each side, up to 0 on the left and top: the window's edges
	pub fn outset(self, edges: Edges<u32>) -> Self {
		let (x, y) = (self.x.saturating_sub(edges.left), self.y.saturating_sub(edges.top));
		Self { x, y, width: self.right() + edges.right - x, height: self.bottom() + edges.bottom - y }
	}

	/// The same rectangle in floating-point numbers, as the GPU takes it
	pub fn to_f32(self) -> Rect<f32> {
		Rect { x: self.x as f32, y: self.y as f32, width: self.width as f32, height: self.height as f32 }
	}
}

impl Rect<f32> {
	/// The rectangle grown by `edges` on each side
	pub fn outset(self, edges: Edges<f32>) -> Self {
		let (width, height) = (edges.left + self.width + edges.right, edges.top + self.height + edges.bottom);
		Self { x: self.x - edges.left, y: self.y - edges.top, width, height }
	}
}

impl<T> From<[T; 4]> for Rect<T> {
	/// From left, top, width, height
	fn from([x, y, width, height]: [T; 4]) -> Self {
		Self { x, y, width, height }
	}
}

impl<T> From<Rect<T>> for [T; 4] {
	/// Left, top, width, height: how the GPU's quads take it
	fn from(rect: Rect<T>) -> Self {
		[rect.x, rect.y, rect.width, rect.height]
	}
}

/// A size on each side of a rectangle, like padding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges<T> {
	/// Above
	pub top: T,
	/// On the right
	pub right: T,
	/// Below
	pub bottom: T,
	/// On the left
	pub left: T,
}

impl<T: Copy> Edges<T> {
	/// The same size on every side
	pub fn all(size: T) -> Self {
		Self { top: size, right: size, bottom: size, left: size }
	}

	/// The sizes converted by `f`, side by side
	pub fn map<U>(self, f: impl Fn(T) -> U) -> Edges<U> {
		Edges { top: f(self.top), right: f(self.right), bottom: f(self.bottom), left: f(self.left) }
	}
}

impl<T> From<Edges<T>> for [T; 4] {
	/// Top, right, bottom and left, like CSS
	fn from(edges: Edges<T>) -> Self {
		[edges.top, edges.right, edges.bottom, edges.left]
	}
}

impl<T> From<[T; 4]> for Edges<T> {
	/// From top, right, bottom and left, like CSS
	fn from([top, right, bottom, left]: [T; 4]) -> Self {
		Self { top, right, bottom, left }
	}
}

impl<T: Add<Output = T>> Add for Edges<T> {
	type Output = Self;

	fn add(self, other: Self) -> Self {
		Self {
			top: self.top + other.top,
			right: self.right + other.right,
			bottom: self.bottom + other.bottom,
			left: self.left + other.left,
		}
	}
}
