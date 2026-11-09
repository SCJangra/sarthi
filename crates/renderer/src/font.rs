use std::sync::Arc;

use glyphon::FontSystem;
use glyphon::cosmic_text::skrifa::charmap::Charmap;
use glyphon::cosmic_text::skrifa::instance::{LocationRef, Size};
use glyphon::cosmic_text::skrifa::{FontRef, MetadataProvider};
use glyphon::fontdb::Source;

/// nvui draws text with JetBrains Mono, bundled so it looks the same everywhere (license: `assets/fonts/OFL.txt`).
/// Both faces are variable fonts, so every weight is available.
const FAMILY: &str = "JetBrains Mono";
const REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono[wght].ttf");
const FACES: [&[u8]; 2] = [REGULAR, include_bytes!("../assets/fonts/JetBrainsMono-Italic[wght].ttf")];

/// Size of a grid cell, in physical pixels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
	pub width: u32,
	pub height: u32,
}

/// The fonts text is drawn with: the bundled font, and the system's fonts as fallback for characters it
/// doesn't have (e.g. CJK and emoji)
pub struct Fonts {
	pub(crate) system: FontSystem,

	/// Which characters the bundled font has
	charmap: Charmap<'static>,

	/// Cell width and height at a font size of 1 pixel
	cell_em: (f32, f32),
}

impl Fonts {
	/// Loads the bundled font and finds the system's fonts, which takes a few milliseconds.
	pub fn new() -> Self {
		let mut system = FontSystem::new();
		let db = system.db_mut();
		for face in FACES {
			db.load_font_source(Source::Binary(Arc::new(face)));
		}
		db.set_monospace_family(FAMILY);

		let regular = FontRef::new(REGULAR).expect("the bundled font is valid");
		let cell_em = measure_cell(&regular).expect("the bundled font has metrics");
		Self { system, charmap: regular.charmap(), cell_em }
	}

	/// Size of a grid cell for text of `font_size` physical pixels. The width is the advance of `M`, and the
	/// height fits the font's ascent, descent and line gap. Both are whole pixels, so cells line up with the
	/// pixel grid: the width is rounded, and the height rounded up so descenders aren't cut off.
	pub fn cell_size(&self, font_size: f32) -> CellSize {
		let (width, height) = self.cell_em;
		CellSize {
			width: ((width * font_size).round() as u32).max(1),
			height: ((height * font_size).ceil() as u32).max(1),
		}
	}

	/// Whether the bundled font has every character of `text`. Its glyphs are exactly one cell wide, while
	/// glyphs from fallback fonts can have any width.
	pub(crate) fn covers(&self, text: &str) -> bool {
		text.chars().all(|c| self.charmap.map(c).is_some())
	}
}

impl Default for Fonts {
	fn default() -> Self {
		Self::new()
	}
}

/// Cell width and height of `font` at a font size of 1 pixel, at its default weight (a monospace font is
/// equally wide at every weight)
fn measure_cell(font: &FontRef<'_>) -> Option<(f32, f32)> {
	// In font units, and scaled here: skrifa scales in fixed point, which gives e.g. 0.59996 for 0.6.
	let metrics = font.metrics(Size::unscaled(), LocationRef::default());
	let width = font
		.glyph_metrics(Size::unscaled(), LocationRef::default())
		.advance_width(font.charmap().map('M')?)?;
	let height = metrics.ascent - metrics.descent + metrics.leading;

	let units_per_em = f32::from(metrics.units_per_em);
	Some((width / units_per_em, height / units_per_em))
}

#[cfg(test)]
mod tests {
	use glyphon::fontdb::{Family, Query, Style, Weight};

	use super::{CellSize, FAMILY, Fonts};

	#[test]
	fn cell_size() {
		let fonts = Fonts::new();
		// JetBrains Mono: 600 units wide, ascent 1020, descent 300, no line gap, 1000 units per em.
		assert_eq!(fonts.cell_em, (0.6, 1.32));
		assert_eq!(fonts.cell_size(14.0), CellSize { width: 8, height: 19 });
		assert_eq!(fonts.cell_size(28.0), CellSize { width: 17, height: 37 });
		assert_eq!(fonts.cell_size(0.0), CellSize { width: 1, height: 1 });
	}

	#[test]
	fn covers() {
		let fonts = Fonts::new();
		assert!(fonts.covers("a"));
		assert!(fonts.covers("│─é→λ"));
		assert!(!fonts.covers("漢"));
		assert!(!fonts.covers("😀"));
		assert!(!fonts.covers("a漢"));
	}

	#[test]
	fn monospace_is_the_bundled_font_in_every_style() {
		let fonts = Fonts::new();
		let db = fonts.system.db();

		for (weight, style) in [
			(Weight::NORMAL, Style::Normal),
			(Weight::BOLD, Style::Normal),
			(Weight::NORMAL, Style::Italic),
			(Weight::BOLD, Style::Italic),
		] {
			let id = db
				.query(&Query { families: &[Family::Monospace], weight, style, ..Query::default() })
				.unwrap();
			let face = db.face(id).unwrap();
			// One variable face per style; cosmic-text applies the weight when it shapes and draws text.
			assert_eq!(face.families[0].0, FAMILY);
			assert_eq!(face.style, style);
		}
	}
}
