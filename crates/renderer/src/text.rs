use std::collections::HashMap;
use std::ops::Range;

use glyphon::cosmic_text::Hinting;
use glyphon::{
	Attrs, Buffer, Cache, Color, ColorMode, Family, Metrics, Resolution, Shaping, Style, SwashCache, TextArea,
	TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use rustc_hash::FxHashMap;

use crate::{CellLayer, CellSize, Fonts, Layer, RenderCell, RendererError, TextStyle};

/// Draws the text of cell layers with glyphon
pub(crate) struct TextPipeline {
	atlas: TextAtlas,
	renderer: TextRenderer,
	viewport: Viewport,
	swash_cache: SwashCache,

	/// Font size the rows were laid out at
	font_size: f32,

	/// Laid-out rows of each cell layer, by layer id
	layers: HashMap<u64, Vec<Row>>,
}

/// One row of a cell layer: its cells as last drawn, and their laid-out segments
#[derive(Default)]
struct Row {
	cells: Vec<RenderCell>,
	buffers: Vec<(u32, Buffer)>,
}

impl TextPipeline {
	pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
		let cache = Cache::new(device);
		// Like the clear color: an sRGB surface expects linear colors, other surfaces take sRGB as is.
		let color_mode = if format.is_srgb() { ColorMode::Accurate } else { ColorMode::Web };
		let mut atlas = TextAtlas::with_color_mode(device, queue, &cache, format, color_mode);
		let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

		Self {
			atlas,
			renderer,
			viewport: Viewport::new(device, &cache),
			swash_cache: SwashCache::new(),
			font_size: 0.0,
			layers: HashMap::new(),
		}
	}

	/// Lays out the rows of `layers` that changed since the last frame, with text of `font_size` physical
	/// pixels, and uploads their glyphs.
	pub(crate) fn prepare(
		&mut self,
		device: &wgpu::Device,
		queue: &wgpu::Queue,
		resolution: Resolution,
		fonts: &mut Fonts,
		layers: &[Layer],
		font_size: f32,
	) -> Result<(), RendererError> {
		let cell = fonts.cell_size(font_size);
		if font_size != self.font_size {
			self.font_size = font_size;
			self.layers.clear();
		}

		// Forget layers that are gone, e.g. closed floating windows.
		self.layers
			.retain(|id, _| layers.iter().any(|Layer::Cells(layer)| layer.id == *id));
		for Layer::Cells(layer) in layers {
			update_rows(self.layers.entry(layer.id).or_default(), layer, fonts, font_size, cell);
		}

		self.viewport.update(queue, resolution);

		let text_areas = layers.iter().flat_map(|Layer::Cells(layer)| {
			let (left, top) = (layer.col * cell.width, layer.row * cell.height);
			// Clipped to the layer, so e.g. a wide fallback glyph can't spill out of a floating window
			let bounds = TextBounds {
				left: left as i32,
				top: top as i32,
				right: (left + layer.width * cell.width) as i32,
				bottom: (top + layer.height * cell.height) as i32,
			};

			self.layers[&layer.id].iter().zip(0..).flat_map(move |(row, y)| {
				row.buffers.iter().map(move |(col, buffer)| TextArea {
					buffer,
					left: (left + col * cell.width) as f32,
					top: (top + y * cell.height) as f32,
					scale: 1.0,
					bounds,
					default_color: Color::rgb(0xFF, 0xFF, 0xFF),
					custom_glyphs: &[],
				})
			})
		});

		self.renderer.prepare(
			device,
			queue,
			&mut fonts.system,
			&mut self.atlas,
			&self.viewport,
			text_areas,
			&mut self.swash_cache,
		)?;
		Ok(())
	}

	pub(crate) fn render(&self, pass: &mut wgpu::RenderPass<'_>) -> Result<(), RendererError> {
		self.renderer.render(&self.atlas, &self.viewport, pass)?;
		Ok(())
	}

	/// Frees atlas space of glyphs the last frame didn't use
	pub(crate) fn trim(&mut self) {
		self.atlas.trim();
	}
}

/// Lays out the rows of `layer` whose cells changed since `rows` were laid out. A changed row takes over the
/// layout of another changed row that had the same cells, so content that only moved, e.g. after scrolling, isn't
/// laid out again.
fn update_rows(rows: &mut Vec<Row>, layer: &CellLayer, fonts: &mut Fonts, font_size: f32, cell: CellSize) {
	rows.resize_with(layer.height as usize, Row::default);
	let width = (layer.width as usize).max(1);
	let changed: Vec<usize> = (0..rows.len())
		.zip(layer.cells.chunks_exact(width))
		.filter(|&(index, cells)| rows[index].cells != cells)
		.map(|(index, _)| index)
		.collect();

	// Layouts of the changed rows by their old cells. Rows often repeat (blank lines, closing braces), so the same
	// cells can have several.
	let mut moved: FxHashMap<Vec<RenderCell>, Vec<Vec<(u32, Buffer)>>> = FxHashMap::default();
	for &index in &changed {
		let row = std::mem::take(&mut rows[index]);
		moved.entry(row.cells).or_default().push(row.buffers);
	}

	for index in changed {
		let cells = &layer.cells[index * width..(index + 1) * width];
		let buffers = moved.get_mut(cells).and_then(Vec::pop).unwrap_or_else(|| {
			segments(cells, |text| fonts.covers(text))
				.iter()
				.map(|segment| (segment.col, layout(fonts, font_size, cell, segment)))
				.collect()
		});
		rows[index] = Row { cells: cells.to_vec(), buffers };
	}
}

/// A run of cells laid out together, starting at column `col`
#[derive(Debug, PartialEq)]
struct Segment {
	col: u32,
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

/// Splits a row into segments that each start at their own column. With hinting, glyphs of the bundled font
/// advance exactly one cell, so a segment only ends after a cell that may not: a double-width character, or
/// one `covered` says the bundled font lacks, which a fallback font with other widths draws.
///
/// Spaces draw nothing (cell backgrounds are drawn separately), so segments start and end with text. Leaving
/// out the spaces that fill the rest of a row halves the time to lay it out.
fn segments(cells: &[RenderCell], covered: impl Fn(&str) -> bool) -> Vec<Segment> {
	let mut segments = Vec::new();
	let mut current: Option<Segment> = None;
	// Spaces after the current segment's text, added once more text follows
	let mut spaces = 0;

	for (col, cell) in (0..).zip(cells) {
		match cell.text.as_str() {
			// The right half of a double-width character: the next segment starts after it.
			"" => continue,
			" " => {
				spaces += 1;
				continue;
			},
			_ => (),
		}

		if let Some(segment) = &mut current {
			segment.push_spaces(spaces);
		}
		spaces = 0;

		let segment = current.get_or_insert_with(|| Segment { col, text: String::new(), spans: Vec::new() });
		segment.push(&cell.text, cell.style);

		let double_width = cells.get(col as usize + 1).is_some_and(|next| next.text.is_empty());
		if double_width || !covered(&cell.text) {
			segments.extend(current.take());
		}
	}

	segments.extend(current);
	segments
}

fn layout(fonts: &mut Fonts, font_size: f32, cell: CellSize, segment: &Segment) -> Buffer {
	let mut buffer = Buffer::new(&mut fonts.system, Metrics::new(font_size, cell.height as f32));
	buffer.set_wrap(Wrap::None);
	// Rounds advances to whole pixels, like the cell width, so the bundled font's glyphs line up with cells.
	buffer.set_hinting(Hinting::Enabled);

	let attrs = Attrs::new().family(Family::Monospace);
	let spans = segment
		.spans
		.iter()
		.map(|(range, style)| (&segment.text[range.clone()], text_attrs(&attrs, style)));
	buffer.set_rich_text(spans, &attrs, Shaping::Advanced, None);
	buffer.shape_until_scroll(&mut fonts.system, false);
	buffer
}

fn text_attrs<'a>(attrs: &Attrs<'a>, style: &TextStyle) -> Attrs<'a> {
	let [_, r, g, b] = style.foreground.to_be_bytes();
	attrs
		.clone()
		.color(Color::rgb(r, g, b))
		.weight(if style.bold { Weight::BOLD } else { Weight::NORMAL })
		.style(if style.italic { Style::Italic } else { Style::Normal })
}

#[cfg(test)]
mod tests {
	use arrayvec::ArrayString;

	use super::{CellLayer, Fonts, RenderCell, Segment, TextStyle, segments, update_rows};
	use crate::Decoration;

	const PLAIN: TextStyle = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
	const BOLD: TextStyle = TextStyle { bold: true, ..PLAIN };

	fn cells(text: &[&str], style: TextStyle) -> Vec<RenderCell> {
		text.iter()
			.map(|text| RenderCell {
				text: ArrayString::from(text).unwrap(),
				style,
				background: 0,
				decoration: Decoration::default(),
			})
			.collect()
	}

	fn segment(col: u32, text: &str, spans: &[(std::ops::Range<usize>, TextStyle)]) -> Segment {
		Segment { col, text: text.to_owned(), spans: spans.to_vec() }
	}

	fn ascii_only(text: &str) -> bool {
		text.is_ascii()
	}

	/// A layer one cell wide, with one row per string
	fn column(rows: &[&str]) -> CellLayer {
		CellLayer {
			id: 1,
			row: 0,
			col: 0,
			width: 1,
			height: rows.len() as u32,
			cells: cells(rows, PLAIN),
			cursor: None,
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
		assert_eq!(segments(&row, |_| true), [segment(0, "a漢", &[(0..4, PLAIN)]), segment(3, "b", &[(0..1, PLAIN)])],);
	}

	#[test]
	fn uncovered_character_ends_a_segment() {
		let row = cells(&["a", "é", "b"], PLAIN);
		assert_eq!(segments(&row, ascii_only), [segment(0, "aé", &[(0..3, PLAIN)]), segment(2, "b", &[(0..1, PLAIN)])],);
	}

	#[test]
	fn multibyte_spans_use_byte_ranges() {
		let mut row = cells(&["é"], BOLD);
		row.extend(cells(&["a"], PLAIN));
		assert_eq!(segments(&row, |_| true), [segment(0, "éa", &[(0..2, BOLD), (2..3, PLAIN)])]);
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
	fn spaces_after_a_segment_end_are_left_out() {
		let row = cells(&["漢", "", " ", "b"], PLAIN);
		assert_eq!(segments(&row, |_| true), [segment(0, "漢", &[(0..3, PLAIN)]), segment(3, "b", &[(0..1, PLAIN)])]);
	}

	#[test]
	fn moved_rows_keep_their_layout() {
		let mut fonts = Fonts::new();
		let cell = fonts.cell_size(14.0);
		let mut rows = Vec::new();
		update_rows(&mut rows, &column(&["a", "b", "c"]), &mut fonts, 14.0, cell);
		let laid_out: Vec<_> = rows.iter().map(|row| row.buffers.as_ptr()).collect();

		// Scrolled by one row: "b" and "c" moved up and keep their buffers, "d" is new.
		update_rows(&mut rows, &column(&["b", "c", "d"]), &mut fonts, 14.0, cell);
		let texts: Vec<_> = rows.iter().map(|row| row.cells[0].text.as_str()).collect();
		assert_eq!(texts, ["b", "c", "d"]);
		assert_eq!(rows[0].buffers.as_ptr(), laid_out[1]);
		assert_eq!(rows[1].buffers.as_ptr(), laid_out[2]);
		assert_eq!(rows[2].buffers.len(), 1);
	}

	#[test]
	fn repeated_rows_each_keep_a_layout() {
		let mut fonts = Fonts::new();
		let cell = fonts.cell_size(14.0);
		let mut rows = Vec::new();
		update_rows(&mut rows, &column(&["a", "}", "}"]), &mut fonts, 14.0, cell);
		let mut braces = vec![rows[1].buffers.as_ptr(), rows[2].buffers.as_ptr()];

		update_rows(&mut rows, &column(&["}", "}", "b"]), &mut fonts, 14.0, cell);
		let mut moved = vec![rows[0].buffers.as_ptr(), rows[1].buffers.as_ptr()];
		braces.sort();
		moved.sort();
		assert_eq!(moved, braces);
	}
}
