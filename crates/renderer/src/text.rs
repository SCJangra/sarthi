use std::collections::HashMap;
use std::ops::Range;

use glyphon::cosmic_text::{Hinting, SwashContent};
use glyphon::{
	Attrs, Buffer, Cache, Color, ColorMode, ContentType, CustomGlyph, Family, Metrics, RasterizedCustomGlyph,
	Resolution, Shaping, Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use rustc_hash::FxHashMap;

use crate::builtin_glyphs::{builtin_char, is_builtin, rasterize};
use crate::font::LineMetrics;
use crate::font::Run;
use crate::quad::underline_band;
use crate::renderer::{Piece, PieceKind};
use crate::{CellLayer, CellSize, Fonts, RenderCell, RendererError, TextSize, TextStyle};

/// Draws the text of cell layers with glyphon
pub(crate) struct TextPipeline {
	atlas: TextAtlas,

	/// One per piece, by its index in the frame: glyphon prepares and draws each separately, so pieces can be drawn
	/// one after another
	renderers: Vec<TextRenderer>,

	viewport: Viewport,
	swash_cache: SwashCache,

	/// Text size and fonts (their generation) the rows were laid out with
	size: TextSize,
	fonts: u32,

	/// Laid-out rows of each piece, by layer id and which part of the layer it is
	layers: HashMap<(u64, PieceKind), Vec<Row>>,

	/// Text areas need a buffer; this empty one carries a row's custom glyphs.
	empty: Buffer,
}

/// One row of a cell layer: its cells as last drawn, and how they're drawn
#[derive(Default)]
struct Row {
	cells: Vec<RenderCell>,
	layout: RowLayout,
}

/// How a row's cells are drawn: segments of text by starting column, and custom glyphs for the cells sarthi draws
/// itself (box drawing, blocks, powerline), positioned relative to the row
#[derive(Default)]
struct RowLayout {
	buffers: Vec<(u32, Buffer)>,
	glyphs: Vec<CustomGlyph>,

	/// Where underlines break around the text's ink, as sorted x ranges in pixels from the row's left
	ink: Vec<Range<f32>>,
}

impl TextPipeline {
	pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
		let cache = Cache::new(device);
		// Like the clear color: an sRGB surface expects linear colors, other surfaces take sRGB as is.
		let color_mode = if format.is_srgb() { ColorMode::Accurate } else { ColorMode::Web };
		let atlas = TextAtlas::with_color_mode(device, queue, &cache, format, color_mode);

		Self {
			atlas,
			renderers: Vec::new(),
			viewport: Viewport::new(device, &cache),
			swash_cache: SwashCache::new(),
			size: TextSize::new(0.0),
			fonts: 0,
			layers: HashMap::new(),
			empty: Buffer::new_empty(Metrics::new(1.0, 1.0)),
		}
	}

	/// Lays out the rows of `pieces` that changed since the last frame, with text of `size`, and uploads their
	/// glyphs.
	pub(crate) fn prepare(
		&mut self,
		device: &wgpu::Device,
		queue: &wgpu::Queue,
		resolution: Resolution,
		fonts: &mut Fonts,
		pieces: &[Piece],
		size: TextSize,
	) -> Result<(), RendererError> {
		if (size, fonts.generation) != (self.size, self.fonts) {
			(self.size, self.fonts) = (size, fonts.generation);
			self.layers.clear();
		}

		// Forget pieces that are gone, e.g. closed floating windows.
		self.layers.retain(|key, _| pieces.iter().any(|piece| key_of(piece) == *key));
		for piece in pieces {
			let rows = self.layers.entry(key_of(piece)).or_default();
			update_rows(rows, &piece.layer, fonts, size, &mut self.swash_cache);
		}

		self.viewport.update(queue, resolution);

		// Custom glyphs are the cells sarthi draws itself, rasterized once per size.
		let thickness = fonts.line_metrics(size).thickness;
		for (index, piece) in pieces.iter().enumerate() {
			if index == self.renderers.len() {
				let renderer = TextRenderer::new(&mut self.atlas, device, wgpu::MultisampleState::default(), None);
				self.renderers.push(renderer);
			}

			let text_areas = text_areas(&self.layers[&key_of(piece)], piece, &self.empty);
			self.renderers[index].prepare_with_custom(
				device,
				queue,
				&mut fonts.system,
				&mut self.atlas,
				&self.viewport,
				text_areas,
				&mut self.swash_cache,
				|request| {
					let c = char::from_u32(u32::from(request.id))?;
					let data = rasterize(c, u32::from(request.width), u32::from(request.height), thickness)?;
					Some(RasterizedCustomGlyph { data, content_type: ContentType::Mask })
				},
			)?;
		}
		Ok(())
	}

	/// Where underlines in row `row` of `piece` break around the text's ink, as of the last prepare (see [`ink`])
	pub(crate) fn ink(&self, piece: &Piece, row: usize) -> &[Range<f32>] {
		let rows = self.layers.get(&key_of(piece));
		rows.and_then(|rows| rows.get(row)).map_or(&[], |row| &row.layout.ink)
	}

	/// Draws the text of the piece at `index` in the frame last prepared.
	pub(crate) fn render(&self, pass: &mut wgpu::RenderPass<'_>, index: usize) -> Result<(), RendererError> {
		if let Some(renderer) = self.renderers.get(index) {
			renderer.render(&self.atlas, &self.viewport, pass)?;
		}
		Ok(())
	}

	/// Frees atlas space of glyphs the last frame didn't use
	pub(crate) fn trim(&mut self) {
		self.atlas.trim();
	}
}

/// Identifies the laid-out rows of `piece` across frames
fn key_of(piece: &Piece) -> (u64, PieceKind) {
	(piece.layer.id, piece.kind)
}

/// The text areas of `piece`, whose rows are laid out in `rows`: a segment of text each, and the custom glyphs of
/// each row in an `empty` buffer
fn text_areas<'a>(rows: &'a [Row], piece: &Piece, empty: &'a Buffer) -> impl Iterator<Item = TextArea<'a>> {
	let cell = piece.layer.cell;
	let (left, top) = (piece.layer.x as f32, piece.layer.y as f32 + piece.offset);
	// Clipped like the rest of the piece, so e.g. a wide fallback glyph can't spill out of a floating window
	let [clip_left, clip_top, width, height] = piece.clip;
	let bounds = TextBounds {
		left: clip_left as i32,
		top: clip_top as i32,
		right: (clip_left + width) as i32,
		bottom: (clip_top + height) as i32,
	};

	rows.iter().zip(0..).flat_map(move |(row, y)| {
		let area = move |col: u32, buffer, custom_glyphs| TextArea {
			buffer,
			left: left + (col * cell.width) as f32,
			top: top + (y * cell.height) as f32,
			scale: 1.0,
			bounds,
			default_color: Color::rgb(0xFF, 0xFF, 0xFF),
			custom_glyphs,
		};
		let glyphs = (!row.layout.glyphs.is_empty()).then(|| area(0, empty, &row.layout.glyphs[..]));
		row.layout
			.buffers
			.iter()
			.map(move |(col, buffer)| area(*col, buffer, &[]))
			.chain(glyphs)
	})
}

/// Lays out the rows of `layer` whose cells changed since `rows` were laid out. A changed row takes over the
/// layout of another changed row that had the same cells, so content that only moved, e.g. after scrolling, isn't
/// laid out again.
fn update_rows(
	rows: &mut Vec<Row>,
	layer: &CellLayer,
	fonts: &mut Fonts,
	size: TextSize,
	swash_cache: &mut SwashCache,
) {
	let (cell, lines) = (fonts.cell_size(size), fonts.line_metrics(size));
	rows.resize_with(layer.height as usize, Row::default);
	let width = (layer.width as usize).max(1);
	let changed: Vec<usize> = (0..rows.len())
		.zip(layer.cells.chunks_exact(width))
		.filter(|&(index, cells)| rows[index].cells != cells)
		.map(|(index, _)| index)
		.collect();

	// Layouts of the changed rows by their old cells. Rows often repeat (blank lines, closing braces), so the same
	// cells can have several.
	let mut moved: FxHashMap<Vec<RenderCell>, Vec<RowLayout>> = FxHashMap::default();
	for &index in &changed {
		let row = std::mem::take(&mut rows[index]);
		moved.entry(row.cells).or_default().push(row.layout);
	}

	for index in changed {
		let cells = &layer.cells[index * width..(index + 1) * width];
		let layout = moved.get_mut(cells).and_then(Vec::pop).unwrap_or_else(|| {
			let buffers: Vec<(u32, Buffer)> = segments(cells, |text| fonts.run_of(text))
				.iter()
				.map(|segment| (segment.col, layout(fonts, size.font_size, cell, segment)))
				.collect();
			let ink = ink(cells, &buffers, cell, lines, fonts, swash_cache);
			RowLayout { buffers, glyphs: builtin_glyphs(cells, cell), ink }
		});
		rows[index] = Row { cells: cells.to_vec(), layout };
	}
}

/// Where the ink of underlined text crosses its underline, as sorted x ranges in pixels from the row's left, widened
/// so the underline keeps clear of the text: underlines break there, like CSS's `text-decoration-skip-ink`, so
/// descenders like those of g and y aren't struck through. Empty unless the line metrics turn ink gaps on.
///
/// The glyphs' images come from `swash_cache`, which glyphon draws them from too, so each is rasterized once.
fn ink(
	cells: &[RenderCell],
	buffers: &[(u32, Buffer)],
	cell: CellSize,
	lines: LineMetrics,
	fonts: &mut Fonts,
	swash_cache: &mut SwashCache,
) -> Vec<Range<f32>> {
	// Off unless asked for; and without underlines, there's nothing to break.
	let Some(clearance) = lines.ink_gaps else { return Vec::new() };
	if cells.iter().all(|cell| cell.decoration.underline.is_none()) {
		return Vec::new();
	}
	// Ink fainter than this is anti-aliasing at the glyph's edge, which the clearance covers.
	const INK: u8 = 64;

	let mut gaps = Vec::new();
	for (col, buffer) in buffers {
		let segment_left = (col * cell.width) as f32;
		for run in buffer.layout_runs() {
			for glyph in run.glyphs {
				let index = ((segment_left + glyph.x) / cell.width as f32) as usize;
				let Some(underline) = cells.get(index).and_then(|cell| cell.decoration.underline) else { continue };
				let (band_top, band_bottom) = underline_band(underline, cell, lines);

				let physical = glyph.physical((segment_left, run.line_y), 1.0);
				let Some(image) = swash_cache.get_image(&mut fonts.system, physical.cache_key) else { continue };
				let placement = image.placement;
				let (width, height) = (placement.width as usize, placement.height as usize);
				let (left, top) = (physical.x + placement.left, physical.y - placement.top);
				let channels = if image.content == SwashContent::Mask { 1 } else { 4 };

				// The image's rows inside the underline's band
				let first = (band_top.floor() as i32 - top).clamp(0, height as i32) as usize;
				let last = (band_bottom.ceil() as i32 - top).clamp(0, height as i32) as usize;
				let inked = |x: usize| {
					(first..last).any(|y| {
						let pixel = (y * width + x) * channels;
						image.data[pixel..pixel + channels].iter().any(|&value| value >= INK)
					})
				};

				// Each run of inked columns is a gap.
				let mut x = 0;
				while x < width {
					if !inked(x) {
						x += 1;
						continue;
					}
					let start = x;
					while x < width && inked(x) {
						x += 1;
					}
					gaps.push((left + start as i32) as f32 - clearance..(left + x as i32) as f32 + clearance);
				}
			}
		}
	}

	// Sorted, with overlapping gaps merged
	gaps.sort_by(|a, b| a.start.total_cmp(&b.start));
	gaps.into_iter().fold(Vec::new(), |mut merged: Vec<Range<f32>>, gap| {
		match merged.last_mut() {
			Some(last) if gap.start <= last.end => last.end = last.end.max(gap.end),
			_ => merged.push(gap),
		}
		merged
	})
}

/// A custom glyph, relative to the row, for each cell sarthi draws itself, in the cell's text color
fn builtin_glyphs(cells: &[RenderCell], cell: CellSize) -> Vec<CustomGlyph> {
	(0..)
		.zip(cells)
		.filter_map(|(col, render_cell)| {
			let c = builtin_char(&render_cell.text)?;
			let [_, r, g, b] = render_cell.style.foreground.to_be_bytes();
			Some(CustomGlyph {
				id: u16::try_from(u32::from(c)).ok()?,
				left: (col * cell.width) as f32,
				top: 0.0,
				width: cell.width as f32,
				height: cell.height as f32,
				color: Some(Color::rgb(r, g, b)),
				snap_to_physical_pixel: true,
				metadata: 0,
			})
		})
		.collect()
}

/// A run of cells laid out together, starting at column `col`, in chosen font `face` (see [`Fonts::face_of`])
#[derive(Debug, PartialEq)]
struct Segment {
	col: u32,
	face: Option<u8>,
	text: String,
	spans: Vec<(Range<usize>, TextStyle)>,
}

impl Segment {
	fn push(&mut self, text: &str, style: TextStyle) {
		let start = self.text.len();
		self.text.push_str(text);

		match self.spans.last_mut() {
			Some((range, last)) if *last == style => range.end = self.text.len(),
			_ => self.spans.push((start..self.text.len(), style)),
		}
	}

	/// Adds `count` spaces in the style of the text before them. Spaces draw nothing, so this keeps spans long.
	fn push_spaces(&mut self, count: usize) {
		self.text.extend(std::iter::repeat_n(' ', count));
		if let Some((range, _)) = self.spans.last_mut() {
			range.end = self.text.len();
		}
	}
}

/// Splits a row into segments that each start at their own column. With hinting, most glyphs of the main font advance
/// exactly one cell, so a segment of its text only ends after a cell that may not: a double-width character. A cell
/// whose glyph may have another width, as `run_of` says, is a segment of its own: one another font draws, or one the
/// main font draws wider than a cell.
///
/// Spaces draw nothing (cell backgrounds are drawn separately), so segments start and end with text. Leaving
/// out the spaces that fill the rest of a row halves the time to lay it out.
fn segments(cells: &[RenderCell], mut run_of: impl FnMut(&str) -> Run) -> Vec<Segment> {
	let mut segments = Vec::new();
	let mut current: Option<Segment> = None;
	// Spaces after the current segment's text, added once more text follows
	let mut spaces = 0;

	for (col, cell) in (0..).zip(cells) {
		match cell.text.as_str() {
			// The right half of a double-width character: the next segment starts after it.
			"" => continue,
			// Cells sarthi draws itself count as spaces for the text.
			text if text == " " || is_builtin(text) => {
				spaces += 1;
				continue;
			},
			_ => (),
		}

		if let Run::Alone(face) = run_of(&cell.text) {
			segments.extend(current.take());
			spaces = 0;
			let mut segment = Segment { col, face, text: String::new(), spans: Vec::new() };
			segment.push(&cell.text, cell.style);
			segments.push(segment);
			continue;
		}

		if let Some(segment) = &mut current {
			segment.push_spaces(spaces);
		}
		spaces = 0;

		let segment =
			current.get_or_insert_with(|| Segment { col, face: Some(0), text: String::new(), spans: Vec::new() });
		segment.push(&cell.text, cell.style);

		let double_width = cells.get(col as usize + 1).is_some_and(|next| next.text.is_empty());
		if double_width {
			segments.extend(current.take());
		}
	}

	segments.extend(current);
	segments
}

/// Lays out `segment` in its font, in lines a cell high, with all text bold or italic if the fonts were chosen so. A
/// glyph wider than its cells keeps its size, and draws over the cells after it.
fn layout(fonts: &mut Fonts, font_size: f32, cell: CellSize, segment: &Segment) -> Buffer {
	let family = fonts.family(segment.face).to_owned();
	let (bold, italic) = (fonts.bold, fonts.italic);
	// The face of the segment's font each span is drawn with
	let variants: Vec<(Weight, Style)> = segment
		.spans
		.iter()
		.map(|(_, style)| fonts.variant(segment.face, style.bold || bold, style.italic || italic))
		.collect();

	let mut buffer = Buffer::new(&mut fonts.system, Metrics::new(font_size, cell.height as f32));
	buffer.set_wrap(Wrap::None);
	// Rounds advances to whole pixels, like the cell width, so the main font's glyphs line up with cells.
	buffer.set_hinting(Hinting::Enabled);

	let attrs = Attrs::new().family(Family::Name(&family));
	let spans = segment
		.spans
		.iter()
		.zip(variants)
		.map(|((range, style), (weight, font_style))| {
			let [_, r, g, b] = style.foreground.to_be_bytes();
			let span = attrs.clone().color(Color::rgb(r, g, b)).weight(weight).style(font_style);
			(&segment.text[range.clone()], span)
		});
	buffer.set_rich_text(spans, &attrs, Shaping::Advanced, None);
	buffer.shape_until_scroll(&mut fonts.system, false);
	buffer
}

#[cfg(test)]
mod tests {
	use arrayvec::ArrayString;

	use super::{CellLayer, Fonts, RenderCell, Run, Segment, TextSize, TextStyle, segments, update_rows};
	use crate::{CellSize, Decoration};

	const PLAIN: TextStyle = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
	const BOLD: TextStyle = TextStyle { bold: true, ..PLAIN };

	fn cells(text: &[&str], style: TextStyle) -> Vec<RenderCell> {
		text.iter()
			.map(|text| RenderCell {
				text: ArrayString::from(text).unwrap(),
				style,
				background: 0,
				blend: 0,
				decoration: Decoration::default(),
			})
			.collect()
	}

	/// A segment in the main font
	fn segment(col: u32, text: &str, spans: &[(std::ops::Range<usize>, TextStyle)]) -> Segment {
		Segment { col, face: Some(0), text: text.to_owned(), spans: spans.to_vec() }
	}

	/// A main font with ASCII only, and no other chosen fonts
	fn ascii_only(text: &str) -> Run {
		if text.is_ascii() { Run::Main } else { Run::Alone(None) }
	}

	/// Every character in the main font, one cell wide
	fn main_font(_: &str) -> Run {
		Run::Main
	}

	/// A layer one cell wide, with one row per string
	fn column(rows: &[&str]) -> CellLayer {
		CellLayer {
			id: 1,
			x: 0,
			y: 0,
			cell: CellSize { width: 8, height: 19 },
			width: 1,
			height: rows.len() as u32,
			cells: cells(rows, PLAIN),
			cursor: None,
			scroll: None,
			shadow: None,
		}
	}

	#[test]
	fn plain_row_is_one_segment() {
		let row = cells(&["a", "b", " ", "c"], PLAIN);
		assert_eq!(segments(&row, ascii_only), [segment(0, "ab c", &[(0..4, PLAIN)])]);
	}

	#[test]
	fn equal_styles_merge_into_one_span() {
		let mut row = cells(&["a", "b"], PLAIN);
		row.extend(cells(&["c", "d"], BOLD));
		row.extend(cells(&["e"], PLAIN));
		let spans = [(0..2, PLAIN), (2..4, BOLD), (4..5, PLAIN)];
		assert_eq!(segments(&row, ascii_only), [segment(0, "abcde", &spans)]);
	}

	#[test]
	fn double_width_character_ends_a_segment() {
		let row = cells(&["a", "漢", "", "b"], PLAIN);
		// "漢" is treated as covered here, so only its width ends the segment.
		assert_eq!(segments(&row, main_font), [segment(0, "a漢", &[(0..4, PLAIN)]), segment(3, "b", &[(0..1, PLAIN)])],);
	}

	#[test]
	fn character_of_another_font_is_a_segment_of_its_own() {
		let row = cells(&["a", "b", "é", "c"], PLAIN);
		// "é" is in no chosen font, so the system's fonts draw it.
		let system = Segment { col: 2, face: None, text: String::from("é"), spans: vec![(0..2, PLAIN)] };
		assert_eq!(
			segments(&row, ascii_only),
			[segment(0, "ab", &[(0..2, PLAIN)]), system, segment(3, "c", &[(0..1, PLAIN)])],
		);

		// In the second chosen font
		let face = |text: &str| if text == "é" { Run::Alone(Some(1)) } else { Run::Main };
		assert_eq!(segments(&row, face)[1].face, Some(1));
	}

	#[test]
	fn a_main_font_glyph_wider_than_a_cell_is_a_segment_of_its_own() {
		// Like Iosevka's ➜, two cells wide while Neovim gives it one: in a run, the text after it would move.
		let wide = |text: &str| if text == "➜" { Run::Alone(Some(0)) } else { Run::Main };
		let row = cells(&["e", " ", "➜", " ", "N"], PLAIN);
		let arrow = Segment { col: 2, face: Some(0), text: String::from("➜"), spans: vec![(0..3, PLAIN)] };
		assert_eq!(segments(&row, wide), [segment(0, "e", &[(0..1, PLAIN)]), arrow, segment(4, "N", &[(0..1, PLAIN)])]);
	}

	#[test]
	fn multibyte_spans_use_byte_ranges() {
		let mut row = cells(&["é"], BOLD);
		row.extend(cells(&["a"], PLAIN));
		assert_eq!(segments(&row, main_font), [segment(0, "éa", &[(0..2, BOLD), (2..3, PLAIN)])]);
	}

	#[test]
	fn empty_row() {
		assert_eq!(segments(&[], ascii_only), []);
		assert_eq!(segments(&cells(&[" ", " "], PLAIN), ascii_only), []);
	}

	#[test]
	fn leading_and_trailing_spaces_are_left_out() {
		let row = cells(&[" ", "a", " ", "b", " ", " "], PLAIN);
		assert_eq!(segments(&row, ascii_only), [segment(1, "a b", &[(0..3, PLAIN)])]);
	}

	#[test]
	fn spaces_take_the_style_before_them() {
		let mut row = cells(&["a"], BOLD);
		row.extend(cells(&[" ", " "], PLAIN));
		row.extend(cells(&["b"], BOLD));
		row.extend(cells(&[" "], BOLD));
		row.extend(cells(&["c"], PLAIN));
		assert_eq!(segments(&row, ascii_only), [segment(0, "a  b c", &[(0..5, BOLD), (5..6, PLAIN)])]);
	}

	#[test]
	fn builtin_characters_count_as_spaces() {
		// A status line: text, a powerline separator, more text; and a box border with its contents
		let row = cells(&["a", "\u{E0BC}", "b", "│", " ", "c", "│"], PLAIN);
		assert_eq!(segments(&row, ascii_only), [segment(0, "a b  c", &[(0..6, PLAIN)])]);
		assert_eq!(segments(&cells(&["╭", "─", "╮"], PLAIN), ascii_only), []);
	}

	#[test]
	fn builtin_characters_get_custom_glyphs() {
		let row = cells(&["a", "\u{E0BC}", "b", "█"], PLAIN);
		let glyphs = super::builtin_glyphs(&row, crate::CellSize { width: 8, height: 19 });
		let placed: Vec<_> = glyphs
			.iter()
			.map(|glyph| (glyph.id, glyph.left, glyph.width, glyph.height))
			.collect();
		assert_eq!(placed, [(0xE0BC, 8.0, 8.0, 19.0), (0x2588, 24.0, 8.0, 19.0)]);
		assert!(
			glyphs
				.iter()
				.all(|glyph| glyph.color == Some(glyphon::Color::rgb(0xFF, 0xFF, 0xFF)))
		);
	}

	#[test]
	fn spaces_after_a_segment_end_are_left_out() {
		let row = cells(&["漢", "", " ", "b"], PLAIN);
		assert_eq!(segments(&row, main_font), [segment(0, "漢", &[(0..3, PLAIN)]), segment(3, "b", &[(0..1, PLAIN)])]);
	}

	#[test]
	fn moved_rows_keep_their_layout() {
		let mut fonts = Fonts::new();
		let (size, mut swash) = (TextSize::new(14.0), glyphon::SwashCache::new());
		let mut rows = Vec::new();
		update_rows(&mut rows, &column(&["a", "b", "c"]), &mut fonts, size, &mut swash);
		let laid_out: Vec<_> = rows.iter().map(|row| row.layout.buffers.as_ptr()).collect();

		// Scrolled by one row: "b" and "c" moved up and keep their buffers, "d" is new.
		update_rows(&mut rows, &column(&["b", "c", "d"]), &mut fonts, size, &mut swash);
		let texts: Vec<_> = rows.iter().map(|row| row.cells[0].text.as_str()).collect();
		assert_eq!(texts, ["b", "c", "d"]);
		assert_eq!(rows[0].layout.buffers.as_ptr(), laid_out[1]);
		assert_eq!(rows[1].layout.buffers.as_ptr(), laid_out[2]);
		assert_eq!(rows[2].layout.buffers.len(), 1);
	}

	#[test]
	fn repeated_rows_each_keep_a_layout() {
		let mut fonts = Fonts::new();
		let (size, mut swash) = (TextSize::new(14.0), glyphon::SwashCache::new());
		let mut rows = Vec::new();
		update_rows(&mut rows, &column(&["a", "}", "}"]), &mut fonts, size, &mut swash);
		let mut braces = vec![rows[1].layout.buffers.as_ptr(), rows[2].layout.buffers.as_ptr()];

		update_rows(&mut rows, &column(&["}", "}", "b"]), &mut fonts, size, &mut swash);
		let mut moved = vec![rows[0].layout.buffers.as_ptr(), rows[1].layout.buffers.as_ptr()];
		braces.sort();
		moved.sort();
		assert_eq!(moved, braces);
	}

	#[test]
	fn bold_and_italic_stay_in_the_family() {
		let mut fonts = Fonts::new();
		let choice = crate::FontChoice { families: vec!["Iosevka".into()], ..crate::FontChoice::default() };
		if fonts.choose(&choice).is_err() {
			eprintln!("skipped: Iosevka isn't installed");
			return;
		}
		let cell = fonts.cell_size(TextSize::new(16.0));

		// Iosevka is often installed without a Bold (700) face, only Regular and Heavy. Bold text must still be
		// drawn in Iosevka, not in another family that has a Bold face.
		for (bold, italic) in [(true, false), (false, true), (true, true)] {
			let style = TextStyle { bold, italic, ..PLAIN };
			let segment = Segment { col: 0, face: Some(0), text: String::from("af"), spans: vec![(0..2, style)] };
			let buffer = super::layout(&mut fonts, 16.0, cell, &segment);
			for glyph in buffer.layout_runs().flat_map(|run| run.glyphs) {
				let face = fonts.system.db().face(glyph.font_id).unwrap();
				assert_eq!(face.families[0].0, "Iosevka", "bold {bold}, italic {italic}: {}", face.post_script_name);
				assert_eq!(face.style == glyphon::fontdb::Style::Italic, italic, "{}", face.post_script_name);
			}
		}
	}

	/// A one-row layer of `text`, one character per cell, underlined in style `underline`
	fn underlined(text: &str, underline: crate::Underline) -> CellLayer {
		let decoration = Decoration { underline: Some(underline), ..Decoration::default() };
		let cells: Vec<RenderCell> = text
			.chars()
			.map(|c| RenderCell { decoration, ..cells(&[&c.to_string()], PLAIN)[0] })
			.collect();
		CellLayer { width: cells.len() as u32, height: 1, cells, ..column(&["x"]) }
	}

	#[test]
	fn underlines_break_around_descenders() {
		let mut fonts = Fonts::new();
		let mut swash = glyphon::SwashCache::new();
		// Ink gaps are off unless turned on.
		let underlines = crate::UnderlineSize { ink_gaps: Some(1.0), ..crate::UnderlineSize::default() };
		let size = TextSize { underlines, ..TextSize::new(14.0) };
		let cell = fonts.cell_size(size);
		let width = cell.width as f32;

		// "g" and "y" reach below the baseline; "a" and "e" don't. Each case gives the gaps' cells.
		let gaps = |fonts: &mut Fonts, swash: &mut glyphon::SwashCache, text: &str, underline, lines| {
			let layer = underlined(text, underline);
			let buffers: Vec<_> = segments(&layer.cells, |text| fonts.run_of(text))
				.iter()
				.map(|segment| (segment.col, super::layout(fonts, size.font_size, cell, segment)))
				.collect();
			let ink = super::ink(&layer.cells, &buffers, cell, lines, fonts, swash);
			ink.iter()
				.map(|gap| ((gap.start / width) as u32, ((gap.end - 1.0) / width) as u32))
				.collect::<Vec<_>>()
		};

		// An underline across the descenders, like some fonts place it: it breaks at g and y only.
		let lines = crate::font::LineMetrics {
			underline_top: fonts.line_metrics(size).underline_top - 2.0,
			..fonts.line_metrics(size)
		};
		assert_eq!(gaps(&mut fonts, &mut swash, "agey", crate::Underline::Single, lines), [(1, 1), (3, 3)]);
		assert_eq!(gaps(&mut fonts, &mut swash, "ae", crate::Underline::Single, lines), []);

		// The bundled font's own underline stays just below its descenders at 14 px, but an undercurl's taller wave
		// crosses them.
		let lines = fonts.line_metrics(size);
		assert_eq!(gaps(&mut fonts, &mut swash, "agey", crate::Underline::Single, lines), []);
		assert_eq!(gaps(&mut fonts, &mut swash, "agey", crate::Underline::Curly, lines), [(1, 1), (3, 3)]);

		// Laid-out rows keep their gaps; without underlines, nothing is looked at.
		let mut rows = Vec::new();
		update_rows(&mut rows, &underlined("agey", crate::Underline::Curly), &mut fonts, size, &mut swash);
		assert_eq!(rows[0].layout.ink.len(), 2);
		update_rows(&mut rows, &column(&["g"]), &mut fonts, size, &mut swash);
		assert_eq!(rows[0].layout.ink, []);

		// Off by default
		update_rows(
			&mut rows,
			&underlined("agey", crate::Underline::Curly),
			&mut fonts,
			TextSize::new(14.0),
			&mut swash,
		);
		assert_eq!(rows[0].layout.ink, []);
	}
}
