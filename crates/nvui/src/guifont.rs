//! The `guifont` option (`:help 'guifont'`), read the way GUIs for Neovim read it: font families separated by commas,
//! with options after colons, e.g. `JetBrains Mono,Symbols Nerd Font:h14:b`.

use std::ops::RangeInclusive;

/// What `guifont` asks for
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuiFont {
	/// The main font, then fallbacks for the characters it lacks. Empty for nvui's default font.
	pub families: Vec<String>,

	/// In points (`:h14`); `None` for nvui's default size
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
					_ => return Err(format!("unknown option: {option} (nvui knows h<size>, b and i)")),
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

/// Font sizes the size functions (`require("nvui").set_font_size()` …) accept, in points
const SIZES: RangeInclusive<f32> = 4.0..=96.0;

/// Points in `pixels` logical pixels
pub fn points(pixels: f32) -> f32 {
	pixels / PIXELS_PER_POINT
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

/// The font size functions: each takes the current `guifont`, and gives the value to set it to. A reset gives back
/// the value before the first change, unless `guifont` was set otherwise since.
#[derive(Debug, Default)]
pub struct FontSizes {
	/// The `guifont` before the first change, and the value the last change gave
	changed: Option<(String, String)>,
}

impl FontSizes {
	/// `guifont` at `points`, an error outside [`SIZES`]
	pub fn set(&mut self, guifont: &str, points: f32) -> Result<String, String> {
		if !SIZES.contains(&points) {
			return Err(format!("font size {points} is outside {}..{} points", SIZES.start(), SIZES.end()));
		}
		GuiFont::parse(guifont)?;
		Ok(self.change_to(guifont, with_size(guifont, points)))
	}

	/// `guifont` at its size (or `default`, in points, without one) plus `delta` points, kept within [`SIZES`]
	pub fn change(&mut self, guifont: &str, delta: f32, default: f32) -> Result<String, String> {
		let points = GuiFont::parse(guifont)?.size.unwrap_or(default) + delta;
		let points = points.clamp(*SIZES.start(), *SIZES.end());
		Ok(self.change_to(guifont, with_size(guifont, points)))
	}

	/// `guifont` before the first change, or as it is if it was set otherwise since
	pub fn reset(&mut self, guifont: &str) -> String {
		self.before(guifont)
	}

	fn change_to(&mut self, guifont: &str, changed: String) -> String {
		self.changed = Some((self.before(guifont), changed.clone()));
		changed
	}

	/// The `guifont` before the first change while it's still what the last change gave, otherwise as it is
	fn before(&mut self, guifont: &str) -> String {
		match self.changed.take() {
			Some((before, after)) if after == guifont => before,
			_ => guifont.to_owned(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{FontSizes, GuiFont, with_size};

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
		let mut sizes = FontSizes::default();
		assert_eq!(sizes.set("Iosevka:h14", 16.0).unwrap(), "Iosevka:h16");
		assert_eq!(sizes.change("Iosevka:h16", 1.0, 10.5).unwrap(), "Iosevka:h17");
		// Without a size, from the default
		assert_eq!(sizes.change("Iosevka", -0.5, 10.5).unwrap(), "Iosevka:h10");

		// Kept within the sizes for changes, an error for set()
		assert_eq!(sizes.change("X:h95", 5.0, 10.5).unwrap(), "X:h96");
		assert_eq!(sizes.change("X:h5", -5.0, 10.5).unwrap(), "X:h4");
		assert_eq!(sizes.set("X", 100.0).unwrap_err(), "font size 100 is outside 4..96 points");
		assert_eq!(sizes.set("X:w1", 12.0).unwrap_err(), "unknown option: w1 (nvui knows h<size>, b and i)");
	}

	#[test]
	fn reset_undoes_the_changes() {
		let mut sizes = FontSizes::default();
		let changed = sizes.set("Iosevka:h14", 16.0).unwrap();
		let changed = sizes.change(&changed, 1.0, 10.5).unwrap();
		assert_eq!(sizes.reset(&changed), "Iosevka:h14");
		// Nothing more to undo
		assert_eq!(sizes.reset("Iosevka:h14"), "Iosevka:h14");

		// Set otherwise since the change, the new value stays.
		sizes.set("", 16.0).unwrap();
		assert_eq!(sizes.reset("Fira Code"), "Fira Code");
		assert_eq!(sizes.reset(":h16"), ":h16");

		// A change after `guifont` was set otherwise undoes to that value.
		sizes.set("Iosevka", 16.0).unwrap();
		let changed = sizes.change("Fira Code:h12", 1.0, 10.5).unwrap();
		assert_eq!(sizes.reset(&changed), "Fira Code:h12");
	}

	#[test]
	fn errors() {
		assert_eq!(GuiFont::parse("X:h0").unwrap_err(), "invalid size: h0");
		assert_eq!(GuiFont::parse("X:hbig").unwrap_err(), "invalid size: hbig");
		assert!(GuiFont::parse("X:w500").unwrap_err().starts_with("unknown option: w500"));
	}
}
