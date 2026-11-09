use std::sync::Arc;

use glyphon::FontSystem;
use glyphon::cosmic_text::Font;
use glyphon::cosmic_text::skrifa::instance::{LocationRef, Size};
use glyphon::cosmic_text::skrifa::metrics::Decoration;
use glyphon::cosmic_text::skrifa::{FontRef, MetadataProvider};
use glyphon::fontdb::{Family, Query, Source, Style, Weight};
use rustc_hash::FxHashMap;

/// nvui draws text with JetBrains Mono unless told otherwise, bundled so it looks the same everywhere (license:
/// `assets/fonts/OFL.txt`). Both faces are variable fonts, so every weight is available.
const BUNDLED: &str = "JetBrains Mono";
const FACES: [&[u8]; 2] = [
	include_bytes!("../assets/fonts/JetBrainsMono[wght].ttf"),
	include_bytes!("../assets/fonts/JetBrainsMono-Italic[wght].ttf"),
];

/// Most fonts [`FontChoice::families`] can name
const MAX_FAMILIES: usize = 32;

/// Size of a grid cell, in physical pixels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
	pub width: u32,
	pub height: u32,
}

/// Which fonts to draw text with
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontChoice {
	/// Font families, by name: the first draws all text it has the characters for, the others, in order, the
	/// characters it lacks, and the system's fonts what none of them has. Empty for the bundled font.
	pub families: Vec<String>,

	/// Draws all text bold or italic, not only text highlighted so
	pub bold: bool,
	pub italic: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum FontError {
	#[error("no font found of: {0}")]
	NotFound(String),

	#[error("font {0} has no usable metrics")]
	NoMetrics(String),

	#[error("at most {MAX_FAMILIES} fonts")]
	TooMany,
}

/// The fonts text is drawn with: the chosen ones (the bundled font unless chosen otherwise), and the system's fonts as
/// fallback for characters they don't have (e.g. CJK and emoji)
pub struct Fonts {
	pub(crate) system: FontSystem,

	/// The chosen fonts, in order: the main font first
	faces: Vec<Face>,

	/// Which of `faces` have each character seen so far, as a bit per face
	coverage: FxHashMap<char, u32>,

	/// Metrics of the main font at a font size of 1 pixel
	em: EmMetrics,

	pub(crate) bold: bool,
	pub(crate) italic: bool,

	/// Changes whenever the fonts do, so text laid out with the old ones is laid out again
	pub(crate) generation: u32,
}

/// A chosen font: the regular face of a family
struct Face {
	/// The family's name as the font spells it
	family: String,
	font: Arc<Font>,

	/// Index of the face in its font file
	index: u32,
}

impl Face {
	fn has(&self, c: char) -> bool {
		FontRef::from_index(self.font.data(), self.index).is_ok_and(|font| font.charmap().map(c).is_some())
	}
}

impl Fonts {
	/// Loads the bundled font and finds the system's fonts, which takes a few milliseconds.
	pub fn new() -> Self {
		let mut system = FontSystem::new();
		for face in FACES {
			system.db_mut().load_font_source(Source::Binary(Arc::new(face)));
		}

		let bundled = find(&mut system, BUNDLED).expect("the bundled font is loaded");
		let em = bundled.metrics().expect("the bundled font has metrics");
		let mut fonts = Self {
			system,
			faces: Vec::new(),
			coverage: FxHashMap::default(),
			em,
			bold: false,
			italic: false,
			generation: 0,
		};
		fonts.use_faces(vec![bundled], em, &FontChoice::default());
		fonts
	}

	/// Draws text with the fonts of `choice`. Families that aren't installed are skipped, like in a list of
	/// alternatives; `monospace` is the bundled font. If none is installed, or the main font can't be used, nothing
	/// changes.
	pub fn choose(&mut self, choice: &FontChoice) -> Result<(), FontError> {
		if choice.families.len() > MAX_FAMILIES {
			return Err(FontError::TooMany);
		}
		let families: Vec<&str> = match choice.families.as_slice() {
			[] => vec![BUNDLED],
			families => families.iter().map(String::as_str).collect(),
		};
		let faces: Vec<Face> = families
			.iter()
			.map(|family| if family.eq_ignore_ascii_case("monospace") { BUNDLED } else { family })
			.filter_map(|family| find(&mut self.system, family))
			.collect();
		let Some(main) = faces.first() else { return Err(FontError::NotFound(families.join(", "))) };
		let em = main.metrics().ok_or_else(|| FontError::NoMetrics(main.family.clone()))?;

		self.use_faces(faces, em, choice);
		Ok(())
	}

	fn use_faces(&mut self, faces: Vec<Face>, em: EmMetrics, choice: &FontChoice) {
		// Text in the monospace family is the main font's.
		self.system.db_mut().set_monospace_family(faces[0].family.clone());
		(self.faces, self.em, self.bold, self.italic) = (faces, em, choice.bold, choice.italic);
		self.coverage.clear();
		self.generation = self.generation.wrapping_add(1);
	}

	/// Size of a grid cell for text of `font_size` physical pixels. The width is the advance of `M`, and the
	/// height fits the font's ascent, descent and line gap. Both are whole pixels, so cells line up with the
	/// pixel grid: the width is rounded, and the height rounded up so descenders aren't cut off.
	pub fn cell_size(&self, font_size: f32) -> CellSize {
		CellSize {
			width: ((self.em.cell_width * font_size).round() as u32).max(1),
			height: ((self.em.cell_height * font_size).ceil() as u32).max(1),
		}
	}

	/// Distance of the baseline from the top of a cell, like cosmic-text places it: the font's ascent and descent
	/// are centered in the line.
	fn baseline(&self, font_size: f32) -> f32 {
		let cell_height = self.cell_size(font_size).height as f32;
		(cell_height - (self.em.ascent + self.em.descent) * font_size) / 2.0 + self.em.ascent * font_size
	}

	/// Where underlines and strikethroughs go in a cell, for text of `font_size` physical pixels
	pub(crate) fn line_metrics(&self, font_size: f32) -> LineMetrics {
		let em = &self.em;
		let cell_height = self.cell_size(font_size).height as f32;
		let baseline = self.baseline(font_size);

		// Whole pixels, so lines are sharp, and inside the cell
		let line = |(offset, thickness): (f32, f32)| {
			let thickness = (thickness * font_size).round().clamp(1.0, cell_height);
			let top = (baseline - offset * font_size).round().clamp(0.0, cell_height - thickness);
			(top, thickness)
		};
		let (underline_top, thickness) = line(em.underline);
		let (strikeout_top, strikeout_thickness) = line(em.strikeout);
		LineMetrics { underline_top, thickness, strikeout_top, strikeout_thickness }
	}

	/// Which chosen font draws `text`, one cell's characters: the index of the first that has them all, or `None` if
	/// none has, for the system's fonts. The main font's glyphs are exactly one cell wide, while other fonts' glyphs
	/// can have any width.
	pub(crate) fn face_of(&mut self, text: &str) -> Option<u8> {
		let faces = &self.faces;
		let coverage = text.chars().fold(u32::MAX, |coverage, c| {
			coverage
				& *self.coverage.entry(c).or_insert_with(|| {
					(0..)
						.zip(faces)
						.filter(|(_, face)| face.has(c))
						.fold(0, |bits, (index, _)| bits | 1 << index)
				})
		});
		(coverage != 0).then(|| coverage.trailing_zeros() as u8)
	}

	/// The family of chosen font `face` (see [`Fonts::face_of`]), or of the main font for `None`, which the
	/// system's fonts stand in for where it lacks characters
	pub(crate) fn family(&self, face: Option<u8>) -> &str {
		&self.faces[usize::from(face.unwrap_or(0)).min(self.faces.len() - 1)].family
	}
}

impl Face {
	/// Metrics at a font size of 1 pixel
	fn metrics(&self) -> Option<EmMetrics> {
		EmMetrics::measure(&FontRef::from_index(self.font.data(), self.index).ok()?)
	}
}

/// The regular face of `family`, whose name may differ in case from the font's
fn find(system: &mut FontSystem, family: &str) -> Option<Face> {
	let db = system.db();
	let name = db
		.faces()
		.flat_map(|face| &face.families)
		.find(|(name, _)| name.eq_ignore_ascii_case(family))
		.map(|(name, _)| name.clone())?;

	let query =
		Query { families: &[Family::Name(&name)], weight: Weight::NORMAL, style: Style::Normal, ..Query::default() };
	let id = db.query(&query)?;
	let index = db.face(id)?.index;
	let font = system.get_font(id, Weight::NORMAL)?;
	Some(Face { family: name, font, index })
}

impl Default for Fonts {
	fn default() -> Self {
		Self::new()
	}
}

/// Where underlines and strikethroughs go in a cell, in pixels from its top
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineMetrics {
	pub underline_top: f32,

	/// Thickness of underlines and overlines
	pub thickness: f32,

	pub strikeout_top: f32,
	pub strikeout_thickness: f32,
}

/// Metrics of a font at a font size of 1 pixel, at its default weight (a monospace font is equally wide at every
/// weight). Distances from the baseline are positive upwards.
#[derive(Debug, Clone, Copy, PartialEq)]
struct EmMetrics {
	/// Advance of `M`
	cell_width: f32,

	/// Ascent, descent and line gap
	cell_height: f32,

	ascent: f32,

	/// Below the baseline, as a positive distance
	descent: f32,

	/// Offset of the top edge from the baseline, and thickness
	underline: (f32, f32),
	strikeout: (f32, f32),
}

impl EmMetrics {
	fn measure(font: &FontRef<'_>) -> Option<Self> {
		// In font units, and scaled here: skrifa scales in fixed point, which gives e.g. 0.59996 for 0.6.
		let metrics = font.metrics(Size::unscaled(), LocationRef::default());
		let width = font
			.glyph_metrics(Size::unscaled(), LocationRef::default())
			.advance_width(font.charmap().map('M')?)?;
		let em = f32::from(metrics.units_per_em);
		let decoration = |decoration: Decoration| (decoration.offset / em, decoration.thickness / em);

		Some(Self {
			cell_width: width / em,
			cell_height: (metrics.ascent - metrics.descent + metrics.leading) / em,
			ascent: metrics.ascent / em,
			descent: -metrics.descent / em,
			underline: decoration(metrics.underline?),
			strikeout: decoration(metrics.strikeout?),
		})
	}
}

#[cfg(test)]
mod tests {
	use glyphon::fontdb::{Family, Query, Style, Weight};
	use glyphon::{Attrs, Buffer, Metrics, Shaping};

	use super::{BUNDLED, CellSize, FontChoice, FontError, Fonts, LineMetrics};

	#[test]
	fn cell_size() {
		let fonts = Fonts::new();
		// JetBrains Mono: 600 units wide, ascent 1020, descent 300, no line gap, 1000 units per em.
		assert_eq!((fonts.em.cell_width, fonts.em.cell_height), (0.6, 1.32));
		assert_eq!(fonts.cell_size(14.0), CellSize { width: 8, height: 19 });
		assert_eq!(fonts.cell_size(28.0), CellSize { width: 17, height: 37 });
		assert_eq!(fonts.cell_size(0.0), CellSize { width: 1, height: 1 });
	}

	#[test]
	fn line_metrics() {
		let fonts = Fonts::new();
		// JetBrains Mono: underline 155 units below the baseline, strikeout 320 above, both 50 thick
		assert_eq!((fonts.em.underline, fonts.em.strikeout), ((-0.155, 0.05), (0.32, 0.05)));

		// At 14 px the baseline is 14.54 px from the top of a 19 px cell.
		let expected =
			LineMetrics { underline_top: 17.0, thickness: 1.0, strikeout_top: 10.0, strikeout_thickness: 1.0 };
		assert_eq!(fonts.line_metrics(14.0), expected);

		// Lines stay inside the cell, at least 1 px thick.
		let tiny = fonts.line_metrics(1.0);
		assert!(tiny.thickness >= 1.0 && tiny.underline_top + tiny.thickness <= 2.0);
	}

	#[test]
	fn baseline_matches_cosmic_text() {
		let mut fonts = Fonts::new();
		for font_size in [14.0, 28.0] {
			let cell = fonts.cell_size(font_size);
			let mut buffer = Buffer::new(&mut fonts.system, Metrics::new(font_size, cell.height as f32));
			buffer.set_text("x", &Attrs::new().family(glyphon::Family::Monospace), Shaping::Advanced, None);
			buffer.shape_until_scroll(&mut fonts.system, false);

			let line_y = buffer.layout_runs().next().unwrap().line_y;
			assert!((line_y - fonts.baseline(font_size)).abs() < 0.01, "{line_y} at {font_size} px");
		}
	}

	#[test]
	fn face_of() {
		let mut fonts = Fonts::new();
		assert_eq!(fonts.face_of("a"), Some(0));
		assert_eq!(fonts.face_of("│─é→λ"), Some(0));
		assert_eq!(fonts.face_of("漢"), None);
		assert_eq!(fonts.face_of("😀"), None);
		assert_eq!(fonts.face_of("a漢"), None);
	}

	#[test]
	fn choose_by_name_in_any_case() {
		let mut fonts = Fonts::new();
		let generation = fonts.generation;
		let choice = FontChoice { families: vec![String::from("jetbrains MONO")], bold: true, italic: false };
		fonts.choose(&choice).unwrap();

		// The font's own spelling is used, and text laid out before is laid out again.
		assert_eq!(fonts.family(Some(0)), BUNDLED);
		assert!(fonts.bold && !fonts.italic);
		assert_ne!(fonts.generation, generation);
	}

	#[test]
	fn missing_fonts_are_skipped() {
		let mut fonts = Fonts::new();
		let families = vec![String::from("No Such Font"), String::from("monospace"), String::from("Nor This")];
		fonts.choose(&FontChoice { families, ..FontChoice::default() }).unwrap();
		// Only `monospace`, the bundled font, is left.
		assert_eq!(fonts.faces.len(), 1);
		assert_eq!(fonts.family(Some(0)), BUNDLED);
	}

	#[test]
	fn no_installed_font_changes_nothing() {
		let mut fonts = Fonts::new();
		let generation = fonts.generation;
		let choice = FontChoice { families: vec!["No Such Font".into(), "Nor This".into()], ..FontChoice::default() };
		assert!(matches!(fonts.choose(&choice), Err(FontError::NotFound(names)) if names == "No Such Font, Nor This"));
		assert_eq!((fonts.generation, fonts.family(None)), (generation, BUNDLED));
	}

	#[test]
	fn another_main_font_and_fallbacks() {
		let mut fonts = Fonts::new();
		let choice = FontChoice { families: vec!["DejaVu Sans Mono".into(), BUNDLED.into()], ..FontChoice::default() };
		if fonts.choose(&choice).is_err() {
			eprintln!("skipped: DejaVu Sans Mono isn't installed");
			return;
		}

		// Measured from DejaVu Sans Mono: 1233 of 2048 units wide
		assert_eq!(fonts.cell_size(14.0).width, 8);
		assert!((fonts.em.cell_width - 1233.0 / 2048.0).abs() < 1e-6);
		assert_eq!(fonts.family(Some(0)), "DejaVu Sans Mono");
		assert_eq!(fonts.face_of("a"), Some(0));
		// Neither has it: the system's fonts draw it.
		assert_eq!(fonts.face_of("漢"), None);
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
			assert_eq!(face.families[0].0, BUNDLED);
			assert_eq!(face.style, style);
		}
	}
}
