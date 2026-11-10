//! Characters sarthi draws itself instead of taking them from a font: box drawing (U+2500–U+257F), block elements
//! (U+2580–U+259F) and powerline separators (U+E0B0–U+E0BF). They're meant to tile, and a font's glyphs rarely fit
//! the cell exactly, which leaves gaps in boxes and bars. Drawn from the cell size in whole pixels, every line runs
//! from edge to edge at the same position in every cell, so neighbours meet seamlessly.

use zeno::{Cap, Command, Fill, Mask, PathBuilder, Stroke};

/// Whether sarthi draws `text` itself
pub(crate) fn is_builtin(text: &str) -> bool {
	builtin_char(text).is_some()
}

/// The character sarthi draws for `text`, if it draws it
pub(crate) fn builtin_char(text: &str) -> Option<char> {
	let mut chars = text.chars();
	let c = chars.next()?;
	(chars.next().is_none() && matches!(c, '\u{2500}'..='\u{259F}' | '\u{E0B0}'..='\u{E0BF}')).then_some(c)
}

/// Draws `c` as an alpha mask of `width` × `height` pixels, row by row, with light lines `thickness` pixels thick.
/// `None` for characters sarthi doesn't draw.
pub(crate) fn rasterize(c: char, width: u32, height: u32, thickness: f32) -> Option<Vec<u8>> {
	let mut canvas = Canvas::new(width, height, thickness.round().max(1.0) as u32);
	match c {
		'\u{2500}'..='\u{257F}' => canvas.box_drawing(c),
		'\u{2580}'..='\u{259F}' => canvas.block(c),
		'\u{E0B0}'..='\u{E0BF}' => canvas.powerline(c),
		_ => return None,
	}
	Some(canvas.data)
}

/// Line weight of one arm of a box drawing character
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Weight {
	None,
	Light,
	Heavy,
	Double,
}

/// The arms of a box drawing line character, up, right, down, left, from a table entry like `".lh."`: `.` none,
/// `l` light, `h` heavy, `d` double
fn arms(code: &str) -> [Weight; 4] {
	let mut arms = [Weight::None; 4];
	for (arm, c) in arms.iter_mut().zip(code.chars()) {
		*arm = match c {
			'l' => Weight::Light,
			'h' => Weight::Heavy,
			'd' => Weight::Double,
			_ => Weight::None,
		};
	}
	arms
}

/// Arms (up, right, down, left) of the box drawing characters made of lines, by code point
fn line_arms(c: char) -> Option<&'static str> {
	Some(match c {
		'─' => ".l.l",
		'━' => ".h.h",
		'│' => "l.l.",
		'┃' => "h.h.",
		'┌' => ".ll.",
		'┍' => ".hl.",
		'┎' => ".lh.",
		'┏' => ".hh.",
		'┐' => "..ll",
		'┑' => "..lh",
		'┒' => "..hl",
		'┓' => "..hh",
		'└' => "ll..",
		'┕' => "lh..",
		'┖' => "hl..",
		'┗' => "hh..",
		'┘' => "l..l",
		'┙' => "l..h",
		'┚' => "h..l",
		'┛' => "h..h",
		'├' => "lll.",
		'┝' => "lhl.",
		'┞' => "hll.",
		'┟' => "llh.",
		'┠' => "hlh.",
		'┡' => "hhl.",
		'┢' => "lhh.",
		'┣' => "hhh.",
		'┤' => "l.ll",
		'┥' => "l.lh",
		'┦' => "h.ll",
		'┧' => "l.hl",
		'┨' => "h.hl",
		'┩' => "h.lh",
		'┪' => "l.hh",
		'┫' => "h.hh",
		'┬' => ".lll",
		'┭' => ".llh",
		'┮' => ".hll",
		'┯' => ".hlh",
		'┰' => ".lhl",
		'┱' => ".lhh",
		'┲' => ".hhl",
		'┳' => ".hhh",
		'┴' => "ll.l",
		'┵' => "ll.h",
		'┶' => "lh.l",
		'┷' => "lh.h",
		'┸' => "hl.l",
		'┹' => "hl.h",
		'┺' => "hh.l",
		'┻' => "hh.h",
		'┼' => "llll",
		'┽' => "lllh",
		'┾' => "lhll",
		'┿' => "lhlh",
		'╀' => "hlll",
		'╁' => "llhl",
		'╂' => "hlhl",
		'╃' => "hllh",
		'╄' => "hhll",
		'╅' => "llhh",
		'╆' => "lhhl",
		'╇' => "hhlh",
		'╈' => "lhhh",
		'╉' => "hlhh",
		'╊' => "hhhl",
		'╋' => "hhhh",
		'═' => ".d.d",
		'║' => "d.d.",
		'╒' => ".dl.",
		'╓' => ".ld.",
		'╔' => ".dd.",
		'╕' => "..ld",
		'╖' => "..dl",
		'╗' => "..dd",
		'╘' => "ld..",
		'╙' => "dl..",
		'╚' => "dd..",
		'╛' => "l..d",
		'╜' => "d..l",
		'╝' => "d..d",
		'╞' => "ldl.",
		'╟' => "dld.",
		'╠' => "ddd.",
		'╡' => "l.ld",
		'╢' => "d.dl",
		'╣' => "d.dd",
		'╤' => ".dld",
		'╥' => ".ldl",
		'╦' => ".ddd",
		'╧' => "ld.d",
		'╨' => "dl.l",
		'╩' => "dd.d",
		'╪' => "ldld",
		'╫' => "dldl",
		'╬' => "dddd",
		'╴' => "...l",
		'╵' => "l...",
		'╶' => ".l..",
		'╷' => "..l.",
		'╸' => "...h",
		'╹' => "h...",
		'╺' => ".h..",
		'╻' => "..h.",
		'╼' => ".h.l",
		'╽' => "l.h.",
		'╾' => ".l.h",
		'╿' => "h.l.",
		_ => return None,
	})
}

/// Where the lines of two opposite arms (up and down, or left and right) lie across the cell
struct Band {
	/// The pixels all their lines cover, `start..end`
	start: u32,
	end: u32,

	/// Whether either arm is double, and where the first and second line of a double arm start
	double: bool,
	first: u32,
	second: u32,
}

impl Band {
	/// The band of arms `a` and `b` across `size` pixels. Without either, it's where a light line would be.
	fn new(canvas: &Canvas, size: u32, a: Weight, b: Weight) -> Self {
		let thickness = canvas.thickness(a).max(canvas.thickness(b)).max(canvas.light);
		let start = Canvas::centered(size, thickness);
		let first = Canvas::centered(size, 3 * canvas.light);
		Self {
			start,
			end: start + thickness,
			double: a == Weight::Double || b == Weight::Double,
			first,
			second: first + 2 * canvas.light,
		}
	}
}

/// An alpha mask of one cell, drawn in whole pixels
struct Canvas {
	width: u32,
	height: u32,

	/// Thickness of light lines; heavy lines are twice as thick, and double lines are two light lines with a light
	/// line between them
	light: u32,

	data: Vec<u8>,
}

impl Canvas {
	fn new(width: u32, height: u32, light: u32) -> Self {
		Self { width, height, light, data: vec![0; (width * height) as usize] }
	}

	/// Fills the pixels `x0..x1` × `y0..y1` with `alpha`, keeping any more opaque pixels
	fn rect(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, alpha: u8) {
		for y in y0.min(self.height)..y1.min(self.height) {
			for x in x0.min(self.width)..x1.min(self.width) {
				let pixel = &mut self.data[(y * self.width + x) as usize];
				*pixel = (*pixel).max(alpha);
			}
		}
	}

	/// Fills `path`, in pixel coordinates, anti-aliased
	fn fill(&mut self, path: &Vec<Command>) {
		self.render(Mask::new(path).style(Fill::NonZero).size(self.width, self.height).render().0);
	}

	/// Strokes `path`, in pixel coordinates, `width` pixels wide, anti-aliased
	fn stroke(&mut self, path: &Vec<Command>, width: f32) {
		let mut stroke = Stroke::new(width);
		stroke.cap(Cap::Butt);
		self.render(Mask::new(path).style(stroke).size(self.width, self.height).render().0);
	}

	fn render(&mut self, mask: Vec<u8>) {
		for (pixel, alpha) in self.data.iter_mut().zip(mask) {
			*pixel = (*pixel).max(alpha);
		}
	}

	/// Thickness of a line of `weight`, and for double lines the thickness of both lines and their gap
	fn thickness(&self, weight: Weight) -> u32 {
		match weight {
			Weight::None => 0,
			Weight::Light => self.light,
			Weight::Heavy => 2 * self.light,
			Weight::Double => 3 * self.light,
		}
	}

	/// Start of a line of `thickness` centered in `size`, in whole pixels, so it's at the same place in every cell
	fn centered(size: u32, thickness: u32) -> u32 {
		size.saturating_sub(thickness) / 2
	}

	fn box_drawing(&mut self, c: char) {
		if let Some(code) = line_arms(c) {
			return self.lines(arms(code));
		}

		let (light, heavy) = (Weight::Light, Weight::Heavy);
		match c {
			'┄' => self.dashes(true, light, 3),
			'┅' => self.dashes(true, heavy, 3),
			'┆' => self.dashes(false, light, 3),
			'┇' => self.dashes(false, heavy, 3),
			'┈' => self.dashes(true, light, 4),
			'┉' => self.dashes(true, heavy, 4),
			'┊' => self.dashes(false, light, 4),
			'┋' => self.dashes(false, heavy, 4),
			'╌' => self.dashes(true, light, 2),
			'╍' => self.dashes(true, heavy, 2),
			'╎' => self.dashes(false, light, 2),
			'╏' => self.dashes(false, heavy, 2),
			'╭' | '╮' | '╯' | '╰' => self.arc(c),
			'╱' => self.diagonals(true, false),
			'╲' => self.diagonals(false, true),
			'╳' => self.diagonals(true, true),
			_ => (),
		}
	}

	/// Draws arms from the edges to the middle, overlapping there, so junctions have no notches. Where double lines
	/// meet, the outer lines join each other and so do the inner ones (`╔`, `╬`). A single line meeting double lines
	/// that run through stops at the near one (`╧`); otherwise lines reach across (`╪`, `╓`).
	fn lines(&mut self, [up, right, down, left]: [Weight; 4]) {
		let (width, height, light) = (self.width, self.height, self.light);
		// Where the vertical arms lie across x, and the horizontal arms across y
		let columns = Band::new(self, width, up, down);
		let rows = Band::new(self, height, left, right);
		let rows_run_through = rows.double && left != Weight::None && right != Weight::None;
		let columns_run_through = columns.double && up != Weight::None && down != Weight::None;

		// Vertical arms: each line's start and end along y
		for (weight, is_up) in [(up, true), (down, false)] {
			let extent = |side: Weight| match (is_up, side == Weight::Double) {
				(true, true) => (0, rows.first + light),
				(true, false) => (0, rows.end),
				(false, true) => (rows.second, height),
				(false, false) => (rows.start, height),
			};
			match weight {
				Weight::None => (),
				Weight::Double => {
					for (x, side) in [(columns.first, left), (columns.second, right)] {
						let (from, to) = extent(side);
						self.rect(x, from, x + light, to, 255);
					}
				},
				_ => {
					let opposite = if is_up { down } else { up };
					let (from, to) = match (is_up, rows_run_through && opposite == Weight::None) {
						(true, true) => (0, rows.first + light),
						(false, true) => (rows.second, height),
						_ => extent(Weight::None),
					};
					let thickness = self.thickness(weight);
					let x = Self::centered(width, thickness);
					self.rect(x, from, x + thickness, to, 255);
				},
			}
		}

		// Horizontal arms: each line's start and end along x
		for (weight, is_left) in [(left, true), (right, false)] {
			let extent = |side: Weight| match (is_left, side == Weight::Double) {
				(true, true) => (0, columns.first + light),
				(true, false) => (0, columns.end),
				(false, true) => (columns.second, width),
				(false, false) => (columns.start, width),
			};
			match weight {
				Weight::None => (),
				Weight::Double => {
					for (y, side) in [(rows.first, up), (rows.second, down)] {
						let (from, to) = extent(side);
						self.rect(from, y, to, y + light, 255);
					}
				},
				_ => {
					let opposite = if is_left { right } else { left };
					let (from, to) = match (is_left, columns_run_through && opposite == Weight::None) {
						(true, true) => (0, columns.first + light),
						(false, true) => (columns.second, width),
						_ => extent(Weight::None),
					};
					let thickness = self.thickness(weight);
					let y = Self::centered(height, thickness);
					self.rect(from, y, to, y + thickness, 255);
				},
			}
		}
	}

	/// `count` dashes of equal length along the cell, each centered in its share, so they keep their rhythm across
	/// cells
	fn dashes(&mut self, horizontal: bool, weight: Weight, count: u32) {
		let thickness = self.thickness(weight);
		let (length, across) = if horizontal { (self.width, self.height) } else { (self.height, self.width) };
		let offset = Self::centered(across, thickness);
		let dash = (length / (2 * count)).max(1);
		for i in 0..count {
			let (start, end) = (length * i / count, length * (i + 1) / count);
			let from = start + (end - start).saturating_sub(dash) / 2;
			if horizontal {
				self.rect(from, offset, from + dash, offset + thickness, 255);
			} else {
				self.rect(offset, from, offset + thickness, from + dash, 255);
			}
		}
	}

	/// A rounded corner: the middle lines of `─` and `│`, joined by a quarter circle
	fn arc(&mut self, c: char) {
		let (width, height, light) = (self.width as f32, self.height as f32, self.light);
		let x = Self::centered(self.width, light) as f32 + light as f32 / 2.0;
		let y = Self::centered(self.height, light) as f32 + light as f32 / 2.0;
		let radius = (width / 2.0).min(height / 2.0);

		// Edge points of the horizontal and vertical arms, and the corner they turn around
		let (horizontal_end, vertical_end) = match c {
			'╭' => (width, height),
			'╮' => (0.0, height),
			'╯' => (0.0, 0.0),
			_ => (width, 0.0),
		};
		let dx = if horizontal_end > x { radius } else { -radius };
		let dy = if vertical_end > y { radius } else { -radius };

		// A cubic Bézier close to a quarter circle
		const K: f32 = 0.552_284_8;
		let mut path = Vec::new();
		path.move_to([horizontal_end, y])
			.line_to([x + dx, y])
			.curve_to([x + dx * (1.0 - K), y], [x, y + dy * (1.0 - K)], [x, y + dy])
			.line_to([x, vertical_end]);
		self.stroke(&path, light as f32);
	}

	fn diagonals(&mut self, rising: bool, falling: bool) {
		let (width, height) = (self.width as f32, self.height as f32);
		// Past the corners, so the ends are cut by the cell edges and meet the next cell's line
		let (dx, dy) = (width / height, 1.0);
		let mut path = Vec::new();
		if rising {
			path.move_to([width + dx, -dy]).line_to([-dx, height + dy]);
		}
		if falling {
			path.move_to([-dx, -dy]).line_to([width + dx, height + dy]);
		}
		self.stroke(&path, self.light as f32);
	}

	fn block(&mut self, c: char) {
		let (width, height) = (self.width, self.height);
		// `n` eighths of `size`, in whole pixels
		let eighths = |size: u32, n: u32| (size * n + 4) / 8;
		let (mid_x, mid_y) = (eighths(width, 4), eighths(height, 4));
		let code = c as u32;

		match c {
			'▀' => self.rect(0, 0, width, mid_y, 255),
			// Lower one to seven eighths, then the full block
			'▁'..='█' => self.rect(0, height - eighths(height, code - 0x2580), width, height, 255),
			// Left seven eighths down to one eighth
			'▉'..='▏' => self.rect(0, 0, eighths(width, 0x2590 - code), height, 255),
			'▐' => self.rect(mid_x, 0, width, height, 255),
			'░' => self.rect(0, 0, width, height, 64),
			'▒' => self.rect(0, 0, width, height, 128),
			'▓' => self.rect(0, 0, width, height, 191),
			'▔' => self.rect(0, 0, width, eighths(height, 1), 255),
			'▕' => self.rect(width - eighths(width, 1), 0, width, height, 255),
			_ => {
				// Quadrants: upper left, upper right, lower left, lower right
				let quadrants: [bool; 4] = match c {
					'▖' => [false, false, true, false],
					'▗' => [false, false, false, true],
					'▘' => [true, false, false, false],
					'▙' => [true, false, true, true],
					'▚' => [true, false, false, true],
					'▛' => [true, true, true, false],
					'▜' => [true, true, false, true],
					'▝' => [false, true, false, false],
					'▞' => [false, true, true, false],
					_ => [false, true, true, true],
				};
				let boxes = [
					(0, 0, mid_x, mid_y),
					(mid_x, 0, width, mid_y),
					(0, mid_y, mid_x, height),
					(mid_x, mid_y, width, height),
				];
				for ((x0, y0, x1, y1), on) in boxes.into_iter().zip(quadrants) {
					if on {
						self.rect(x0, y0, x1, y1, 255);
					}
				}
			},
		}
	}

	fn powerline(&mut self, c: char) {
		let (w, h) = (self.width as f32, self.height as f32);
		let mid = h / 2.0;
		let triangle = |points: [[f32; 2]; 3]| {
			let mut path = Vec::new();
			path.move_to(points[0]).line_to(points[1]).line_to(points[2]).close();
			path
		};
		let polyline = |points: &[[f32; 2]]| {
			let mut path = Vec::new();
			path.move_to(points[0]);
			for &point in &points[1..] {
				path.line_to(point);
			}
			path
		};
		// Half an ellipse from the top to the bottom edge through `tip`, starting at `x`
		let half_circle = |x: f32, tip: f32| {
			const K: f32 = 0.552_284_8;
			let mut path = Vec::new();
			path.move_to([x, 0.0])
				.curve_to([x + (tip - x) * K, 0.0], [tip, mid * (1.0 - K)], [tip, mid])
				.curve_to([tip, mid + mid * K], [x + (tip - x) * K, h], [x, h]);
			path
		};
		let thin = self.light as f32;

		match c {
			'\u{E0B0}' => self.fill(&triangle([[0.0, 0.0], [w, mid], [0.0, h]])),
			'\u{E0B1}' => self.stroke(&polyline(&[[0.0, 0.0], [w, mid], [0.0, h]]), thin),
			'\u{E0B2}' => self.fill(&triangle([[w, 0.0], [0.0, mid], [w, h]])),
			'\u{E0B3}' => self.stroke(&polyline(&[[w, 0.0], [0.0, mid], [w, h]]), thin),
			'\u{E0B4}' => self.fill(&half_circle(0.0, w)),
			'\u{E0B5}' => self.stroke(&half_circle(0.0, w - thin / 2.0), thin),
			'\u{E0B6}' => self.fill(&half_circle(w, 0.0)),
			'\u{E0B7}' => self.stroke(&half_circle(w, thin / 2.0), thin),
			'\u{E0B8}' => self.fill(&triangle([[0.0, 0.0], [0.0, h], [w, h]])),
			'\u{E0BA}' => self.fill(&triangle([[w, 0.0], [w, h], [0.0, h]])),
			'\u{E0BC}' => self.fill(&triangle([[0.0, 0.0], [w, 0.0], [0.0, h]])),
			'\u{E0BE}' => self.fill(&triangle([[0.0, 0.0], [w, 0.0], [w, h]])),
			// The thin ones are the diagonal of their triangle.
			'\u{E0B9}' | '\u{E0BF}' => self.stroke(&polyline(&[[0.0, 0.0], [w, h]]), thin),
			_ => self.stroke(&polyline(&[[w, 0.0], [0.0, h]]), thin),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{builtin_char, is_builtin, rasterize};

	const W: u32 = 8;
	const H: u32 = 19;

	/// The mask of `c` in an 8×19 cell with 1 px light lines, as rows of booleans (mostly covered pixels)
	fn draw(c: char) -> Vec<Vec<bool>> {
		let mask = rasterize(c, W, H, 1.0).unwrap();
		mask.chunks(W as usize)
			.map(|row| row.iter().map(|&alpha| alpha > 127).collect())
			.collect()
	}

	fn alpha(c: char, x: u32, y: u32) -> u8 {
		rasterize(c, W, H, 1.0).unwrap()[(y * W + x) as usize]
	}

	/// Rows and columns that are covered across the whole cell
	fn full_rows(mask: &[Vec<bool>]) -> Vec<usize> {
		(0..mask.len()).filter(|&y| mask[y].iter().all(|&on| on)).collect()
	}

	fn full_columns(mask: &[Vec<bool>]) -> Vec<usize> {
		(0..W as usize).filter(|&x| mask.iter().all(|row| row[x])).collect()
	}

	#[test]
	fn which_characters() {
		assert!(is_builtin("─") && is_builtin("█") && is_builtin("\u{E0BC}"));
		assert!(!is_builtin("a") && !is_builtin("─a") && !is_builtin("\u{E0C0}") && !is_builtin(""));
		assert_eq!(builtin_char("╭"), Some('╭'));
		assert_eq!(rasterize('a', W, H, 1.0), None);
	}

	#[test]
	fn lines_run_edge_to_edge_in_the_middle() {
		// One row 9 (centered in 19), one column 3 (centered in 8): the same in every cell, so they tile.
		assert_eq!(full_rows(&draw('─')), [9]);
		assert_eq!(full_columns(&draw('│')), [3]);
		assert_eq!(full_rows(&draw('━')), [8, 9]);
		assert_eq!(full_columns(&draw('┃')), [3, 4]);
		assert_eq!(full_rows(&draw('═')), [8, 10]);
		assert_eq!(full_columns(&draw('║')), [2, 4]);
	}

	#[test]
	fn junctions_have_no_gaps() {
		let cross = draw('┼');
		assert_eq!((full_rows(&cross), full_columns(&cross)), (vec![9], vec![3]));

		// The arms of └ meet: the vertical line reaches the horizontal one, which starts at the vertical one.
		let corner = draw('└');
		assert!((0..=9).all(|y| corner[y][3]) && (10..H as usize).all(|y| !corner[y][3]));
		assert!((3..W as usize).all(|x| corner[9][x]) && (0..3).all(|x| !corner[9][x]));
	}

	#[test]
	fn double_corner_nests_its_lines() {
		// ╔: the outer line turns at (2, 8), the inner one at (4, 10).
		let corner = draw('╔');
		assert!((2..W as usize).all(|x| corner[8][x]) && !corner[8][1]);
		assert!((8..H as usize).all(|y| corner[y][2]) && !corner[7][2]);
		assert!((4..W as usize).all(|x| corner[10][x]) && !corner[10][3]);
		assert!((10..H as usize).all(|y| corner[y][4]) && !corner[9][4]);
	}

	#[test]
	fn double_cross_closes_all_four_corners() {
		// ╬: double lines at columns 2, 4 and rows 8, 10; each quarter is an L whose corner pixel is covered.
		let cross = draw('╬');
		for (x, y) in [(2, 8), (4, 8), (2, 10), (4, 10)] {
			assert!(cross[y][x], "corner ({x}, {y})");
		}
		// The gap between the lines stays open, so the four Ls don't touch.
		assert!(!cross[9][3] && !cross[9][0] && !cross[0][3]);
	}

	#[test]
	fn single_lines_meeting_double_ones() {
		// ╧: the single line comes down to the upper double line and stops there.
		let tee = draw('╧');
		assert!(tee[8][3] && !tee[9][3]);
		// ╪: the single line runs straight through both double lines.
		assert!((0..H as usize).all(|y| draw('╪')[y][3]));
		// ╓: the single line reaches across both double lines, which hang from it.
		let corner = draw('╓');
		assert!((2..W as usize).all(|x| corner[9][x]) && !corner[9][1]);
		assert!(corner[10][2] && corner[10][4] && !corner[8][2]);
	}

	#[test]
	fn every_character_draws_at_any_size() {
		let chars = ('\u{2500}'..='\u{259F}').chain('\u{E0B0}'..='\u{E0BF}');
		for (width, height, thickness) in [(1, 1, 1.0), (3, 5, 1.0), (8, 19, 1.0), (17, 37, 2.0), (40, 90, 5.0)] {
			for c in chars.clone() {
				let mask = rasterize(c, width, height, thickness).unwrap();
				assert_eq!(mask.len(), (width * height) as usize);
				if width >= 8 {
					assert!(mask.iter().any(|&alpha| alpha > 0), "U+{:04X} at {width}x{height} is empty", c as u32);
				}
			}
		}
	}

	#[test]
	fn rounded_corners_meet_straight_lines() {
		// ╭ leaves through the right edge on the row of ─ and through the bottom edge on the column of │.
		let corner = draw('╭');
		assert!(corner[9][W as usize - 1] && !corner[8][W as usize - 1] && !corner[10][W as usize - 1]);
		assert!(corner[H as usize - 1][3] && !corner[H as usize - 1][2] && !corner[H as usize - 1][4]);
		let other = draw('╯');
		assert!(other[9][0] && other[0][3]);
	}

	#[test]
	fn dashes() {
		let dashed = draw('┄');
		// Three 1 px dashes along row 9, each centered in its third of the cell
		let row: Vec<bool> = dashed[9].clone();
		assert_eq!(row, [true, false, false, true, false, false, true, false]);
	}

	#[test]
	fn blocks_fill_exact_fractions() {
		assert!(draw('█').iter().all(|row| row.iter().all(|&on| on)));
		// Lower half: rows 9.5 rounded to 10 of 19
		assert_eq!(full_rows(&draw('▄')), (9..19).collect::<Vec<_>>());
		assert_eq!(full_rows(&draw('▀')), (0..10).collect::<Vec<_>>());
		// Lower one eighth (2 px) and left one eighth (1 px)
		assert_eq!(full_rows(&draw('▁')), [17, 18]);
		assert_eq!(full_columns(&draw('▏')), [0]);
		// The halves of ▌ and ▐ add up to the cell without overlapping.
		assert_eq!(full_columns(&draw('▌')), [0, 1, 2, 3]);
		assert_eq!(full_columns(&draw('▐')), [4, 5, 6, 7]);
		assert_eq!((alpha('░', 0, 0), alpha('▒', 0, 0), alpha('▓', 0, 0)), (64, 128, 191));
	}

	#[test]
	fn quadrants() {
		let quadrant = draw('▚');
		assert!(quadrant[0][0] && !quadrant[0][7] && !quadrant[18][0] && quadrant[18][7]);
	}

	#[test]
	fn powerline_triangles_fill_the_cell() {
		// U+E0BC, upper left: the whole top edge and left edge are covered, the opposite corner isn't.
		let upper_left = draw('\u{E0BC}');
		assert!(upper_left[0].iter().take(7).all(|&on| on));
		assert!(upper_left.iter().take(17).all(|row| row[0]));
		assert!(!upper_left[18][7]);

		// U+E0B0, the arrow: the left edge is covered from top to bottom (but its sliver-thin corners), its tip is in
		// the middle of the right edge.
		let arrow = draw('\u{E0B0}');
		assert!(arrow.iter().skip(1).take(17).all(|row| row[0]));
		assert!(arrow[9][7] && !arrow[0][7] && !arrow[18][7]);
	}

	#[test]
	fn powerline_half_circles() {
		let right = draw('\u{E0B4}');
		assert!(right.iter().all(|row| row[0]) && right[9][7] && !right[0][7]);
		let left = draw('\u{E0B6}');
		assert!(left.iter().all(|row| row[7]) && left[9][0] && !left[0][0]);
	}
}
