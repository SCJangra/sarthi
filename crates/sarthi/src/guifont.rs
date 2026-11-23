//! The `guifont` option (`:help 'guifont'`), read the way GUIs for Neovim read it: font families separated by commas,
//! with options after colons, e.g. `JetBrains Mono,Symbols Nerd Font:h14:b`.

use std::ops::RangeInclusive;

/// What `guifont` asks for
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuiFont {
	/// The main font, then fallbacks for the characters it lacks. Empty for sarthi's default font.
	pub families: Vec<String>,

	/// In points (`:h14`); `None` for sarthi's default size
	pub size: Option<f32>,

	/// All text bold (`:b`) or italic (`:i`)
	pub bold: bool,
	pub italic: bool,
}

/// Logical pixels in a point: a point is 1/72 inch, a logical pixel 1/96.
const PIXELS_PER_POINT: f32 = 96.0 / 72.0;

impl GuiFont {
	/// Reads `guifont`. Options may follow any family; underscores in names stand for spaces, like in gVim.
	pub fn parse(value: &str) -> Result<Self, String> {
		let mut font = Self::default();
		for entry in value.split(',') {
			let mut parts = entry.split(':');
			let family = parts.next().unwrap_or_default().trim().replace('_', " ");
			if !family.is_empty() {
				font.families.push(family);
			}

			for option in parts.map(str::trim) {
				match option.split_at_checked(1) {
					Some(("h", size)) => {
						let size = size.parse().ok().filter(|size: &f32| size.is_finite() && *size > 0.0);
						font.size = Some(size.ok_or_else(|| format!("invalid size: {option}"))?);
					},
					Some(("b", "")) => font.bold = true,
					Some(("i", "")) => font.italic = true,
					_ if option.is_empty() => (),
					_ => return Err(format!("unknown option: {option} (sarthi knows h<size>, b and i)")),
				}
			}
		}
		Ok(font)
	}

	/// The size in logical pixels, if `guifont` has one
	pub fn pixels(&self) -> Option<f32> {
		self.size.map(|points| points * PIXELS_PER_POINT)
	}
}

/// Font sizes the size functions (`require("sarthi").set_font_size()` …) accept, in points
const SIZES: RangeInclusive<f32> = 4.0..=96.0;

/// Points in `pixels` logical pixels
pub fn points(pixels: f32) -> f32 {
	pixels / PIXELS_PER_POINT
}

/// Logical pixels in a font size of `points`, if it's a size the font size functions accept too
pub fn pixels(points: f32) -> Result<f32, String> {
	if !SIZES.contains(&points) {
		return Err(format!("font size {points} is outside {}..{} points", SIZES.start(), SIZES.end()));
	}
	Ok(points * PIXELS_PER_POINT)
}

/// `guifont` with its size set to `points`: the first family gets `:h<points>`, any other `:h` goes, and the rest
/// stays as it was.
fn with_size(guifont: &str, points: f32) -> String {
	// To hundredths, so steps like 10.1 + 0.1 don't show as 10.200001
	let points = (points * 100.0).round() / 100.0;
	let entries = guifont.split(',').enumerate().map(|(index, entry)| {
		let mut parts = entry.split(':');
		let mut entry = parts.next().unwrap_or_default().to_owned();
		for option in parts.filter(|option| !option.trim().starts_with('h')) {
			entry = format!("{entry}:{option}");
		}
		if index == 0 {
			entry = format!("{entry}:h{points}");
		}
		entry
	});
	entries.collect::<Vec<_>>().join(",")
}

/// `guifont` at `points`, an error outside [`SIZES`]
pub fn set_size(guifont: &str, points: f32) -> Result<String, String> {
	pixels(points)?;
	GuiFont::parse(guifont)?;
	Ok(with_size(guifont, points))
}

/// `guifont` at its size (or `default`, in points, without one) plus `delta` points, kept within [`SIZES`]
pub fn change_size(guifont: &str, delta: f32, default: f32) -> Result<String, String> {
	let points = GuiFont::parse(guifont)?.size.unwrap_or(default) + delta;
	Ok(with_size(guifont, points.clamp(*SIZES.start(), *SIZES.end())))
}

#[cfg(test)]
mod tests {
	use super::{GuiFont, change_size, set_size, with_size};

	fn families(names: &[&str]) -> Vec<String> {
		names.iter().map(|name| name.to_string()).collect()
	}

	#[test]
	fn empty_means_defaults() {
		assert_eq!(GuiFont::parse("").unwrap(), GuiFont::default());
	}

	#[test]
	fn families_and_options() {
		let font = GuiFont::parse("JetBrains Mono,Symbols Nerd Font:h13.5:b").unwrap();
		let expected = GuiFont {
			families: families(&["JetBrains Mono", "Symbols Nerd Font"]),
			size: Some(13.5),
			bold: true,
			italic: false,
		};
		assert_eq!(font, expected);

		// Options after any family, underscores for spaces, spaces around commas
		let font = GuiFont::parse("Fira_Code:h12, Noto Color Emoji:i").unwrap();
		assert_eq!(font.families, families(&["Fira Code", "Noto Color Emoji"]));
		assert_eq!((font.size, font.italic), (Some(12.0), true));

		// Only a size
		assert_eq!(GuiFont::parse(":h11").unwrap(), GuiFont { size: Some(11.0), ..GuiFont::default() });
	}

	#[test]
	fn sizes_are_points() {
		assert_eq!(GuiFont::parse("X:h12").unwrap().pixels(), Some(16.0));
		assert_eq!(GuiFont::parse("X").unwrap().pixels(), None);
	}

	#[test]
	fn with_size_keeps_the_rest() {
		assert_eq!(with_size("", 11.0), ":h11");
		assert_eq!(with_size("Iosevka", 12.5), "Iosevka:h12.5");
		assert_eq!(with_size("Iosevka:b:h14, Noto Color Emoji:h9:i", 16.0), "Iosevka:b:h16, Noto Color Emoji:i");
		assert_eq!(with_size("X", 10.1 + 0.1), "X:h10.2");
	}

	#[test]
	fn font_sizes_change_guifont() {
		assert_eq!(set_size("Iosevka:h14", 16.0).unwrap(), "Iosevka:h16");
		assert_eq!(change_size("Iosevka:h16", 1.0, 10.5).unwrap(), "Iosevka:h17");
		// Without a size, from the default
		assert_eq!(change_size("Iosevka", -0.5, 10.5).unwrap(), "Iosevka:h10");

		// Kept within the sizes for changes, an error for set_size()
		assert_eq!(change_size("X:h95", 5.0, 10.5).unwrap(), "X:h96");
		assert_eq!(change_size("X:h5", -5.0, 10.5).unwrap(), "X:h4");
		assert_eq!(set_size("X", 100.0).unwrap_err(), "font size 100 is outside 4..96 points");
		assert_eq!(set_size("X:w1", 12.0).unwrap_err(), "unknown option: w1 (sarthi knows h<size>, b and i)");
	}


	#[test]
	fn errors() {
		assert_eq!(GuiFont::parse("X:h0").unwrap_err(), "invalid size: h0");
		assert_eq!(GuiFont::parse("X:hbig").unwrap_err(), "invalid size: hbig");
		assert!(GuiFont::parse("X:w500").unwrap_err().starts_with("unknown option: w500"));
	}
}
