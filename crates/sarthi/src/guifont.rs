//! The `guifont` option (`:help 'guifont'`), read the way GUIs for Neovim read it: font families separated by commas,
//! with options after colons, e.g. `JetBrains Mono,Symbols Nerd Font:h14`.

use std::ops::RangeInclusive;

use rpc::sarthi::{Font, FontFamily, FontSize};

/// Logical pixels in a point: a point is 1/72 inch, a logical pixel 1/96.
const PIXELS_PER_POINT: f32 = 96.0 / 72.0;

/// Reads `guifont` into the main font. Options may follow any family; underscores in names stand for spaces, like in
/// gVim. Without `:h` the size is [`FontSize::Main`]: `guifont` has none yet.
pub fn parse(value: &str) -> Result<Font, String> {
	let (mut families, mut size) = (Vec::new(), FontSize::Main);
	for entry in value.split(',') {
		let mut parts = entry.split(':');
		let family = parts.next().unwrap_or_default().trim().replace('_', " ");
		if !family.is_empty() {
			families.push(family);
		}

		for option in parts.map(str::trim) {
			match option.split_at_checked(1) {
				Some(("h", points)) => {
					let points = points.parse().ok().filter(|points: &f32| points.is_finite() && *points > 0.0);
					size = FontSize::Points(points.ok_or_else(|| format!("invalid size: {option}"))?);
				},
				_ if option.is_empty() => (),
				_ => return Err(format!("unknown option: {option} (sarthi knows h<size>)")),
			}
		}
	}
	Ok(Font { family: FontFamily::Named(families), size })
}

/// Font sizes the size functions (`require("sarthi").set_font_size()` …) and windows' fonts take, in points.
const SIZES: RangeInclusive<f32> = 4.0..=96.0;

/// Points in `pixels` logical pixels.
pub fn points(pixels: f32) -> f32 {
	pixels / PIXELS_PER_POINT
}

/// Logical pixels in a font size of `points`.
pub fn pixels(points: f32) -> f32 {
	points * PIXELS_PER_POINT
}

/// `points`, or what's wrong with it: a size outside [`SIZES`].
pub fn checked(points: f32) -> Result<f32, String> {
	if !SIZES.contains(&points) {
		return Err(format!("font size {points} is outside {}..{} points", SIZES.start(), SIZES.end()));
	}
	Ok(points)
}

/// `points` plus `delta`, kept within [`SIZES`].
pub fn changed(points: f32, delta: f32) -> f32 {
	(points + delta).clamp(*SIZES.start(), *SIZES.end())
}

/// `guifont` with its size set to `points`: the first family gets `:h<points>`, any other `:h` goes, and the rest
/// stays as it was.
pub fn with_size(guifont: &str, points: f32) -> String {
	// To hundredths, so steps like 10.1 + 0.1 don't show as 10.200001.
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

#[cfg(test)]
mod tests {
	use rpc::sarthi::{Font, FontFamily, FontSize};

	use super::{changed, checked, parse, pixels, with_size};

	fn named(names: &[&str]) -> FontFamily {
		FontFamily::Named(names.iter().map(|name| name.to_string()).collect())
	}

	#[test]
	fn empty_means_defaults() {
		assert_eq!(parse("").unwrap(), Font { family: named(&[]), size: FontSize::Main });
	}

	#[test]
	fn families_and_size() {
		let font = parse("JetBrains Mono,Symbols Nerd Font:h13.5").unwrap();
		let expected = Font { family: named(&["JetBrains Mono", "Symbols Nerd Font"]), size: FontSize::Points(13.5) };
		assert_eq!(font, expected);

		// A size after any family, underscores for spaces, spaces around commas.
		let font = parse("Fira_Code, Noto Color Emoji:h12").unwrap();
		assert_eq!(font, Font { family: named(&["Fira Code", "Noto Color Emoji"]), size: FontSize::Points(12.0) });

		// Only a size.
		assert_eq!(parse(":h11").unwrap(), Font { family: named(&[]), size: FontSize::Points(11.0) });
	}

	#[test]
	fn sizes() {
		assert_eq!(pixels(12.0), 16.0);
		assert_eq!(changed(16.0, 1.0), 17.0);
		// Kept within the sizes for changes, an error otherwise.
		assert_eq!((changed(95.0, 5.0), changed(5.0, -5.0)), (96.0, 4.0));
		assert_eq!(checked(100.0).unwrap_err(), "font size 100 is outside 4..96 points");
	}

	#[test]
	fn with_size_keeps_the_rest() {
		assert_eq!(with_size("", 11.0), ":h11");
		assert_eq!(with_size("Iosevka", 12.5), "Iosevka:h12.5");
		assert_eq!(with_size("Iosevka:h14, Noto Color Emoji:h9", 16.0), "Iosevka:h16, Noto Color Emoji");
		assert_eq!(with_size("X", 10.1 + 0.1), "X:h10.2");
	}

	#[test]
	fn errors() {
		assert_eq!(parse("X:h0").unwrap_err(), "invalid size: h0");
		assert_eq!(parse("X:hbig").unwrap_err(), "invalid size: hbig");
		assert_eq!(parse("X:b").unwrap_err(), "unknown option: b (sarthi knows h<size>)");
		assert!(parse("X:w500").unwrap_err().starts_with("unknown option: w500"));
	}
}
