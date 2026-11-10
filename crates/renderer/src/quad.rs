use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use crate::color::rgba;
use crate::font::LineMetrics;
use crate::{
	BarOrientation, CellLayer, CellSize, Cursor, CursorShape, Decoration, LayerShadow, RenderCell, Shadow, Underline,
};

/// A rectangle in physical pixels, with optional rounded corners and border
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct Quad {
	/// Left, top, width, height
	pub rect: [f32; 4],

	/// RGBA as the surface expects it: linear for sRGB surfaces (see [`rgba`]), not premultiplied
	pub color: [f32; 4],
	pub border_color: [f32; 4],

	pub corner_radius: f32,
	pub border_width: f32,

	/// Draws a patterned line instead of a rectangle: kind ([`WAVE`], [`DOTS`] or [`DASHES`]; 0 for none), period and
	/// line thickness in pixels, unused; or a shadow instead ([`SHADOW`]), with its blur radius in pixels
	pub pattern: [f32; 4],
}

const WAVE: f32 = 1.0;
const DOTS: f32 = 2.0;
const DASHES: f32 = 3.0;
const SHADOW: f32 = 4.0;

impl Quad {
	/// A plain rectangle without rounded corners or border
	pub fn fill(rect: [f32; 4], color: [f32; 4]) -> Self {
		Self { rect, color, border_color: [0.0; 4], corner_radius: 0.0, border_width: 0.0, pattern: [0.0; 4] }
	}

	/// `shadow` under a rectangle `rect` (left, top, width, height), for a surface of the given kind: the quad reaches as
	/// far as the shadow does.
	pub fn shadow(rect: [f32; 4], shadow: Shadow, srgb_surface: bool) -> Self {
		// Three standard deviations of the blur, past which the shadow is gone
		let reach = 1.5 * shadow.blur.max(0.0);
		let [left, top, width, height] = rect;
		let [x, y] = shadow.offset;
		let rect = [left + x - reach, top + y - reach, width + 2.0 * reach, height + 2.0 * reach];
		let [r, g, b, _] = rgba(shadow.color, srgb_surface);
		let color = [r, g, b, shadow.opacity.clamp(0.0, 1.0)];
		Self { pattern: [SHADOW, shadow.blur.max(0.0), 0.0, 0.0], ..Self::fill(rect, color) }
	}

	/// A line of `kind` across `rect`, `thickness` pixels thick, repeating every `period` pixels
	fn patterned(rect: [f32; 4], color: [f32; 4], kind: f32, period: f32, thickness: f32) -> Self {
		Self { pattern: [kind, period, thickness, 0.0], ..Self::fill(rect, color) }
	}
}

/// Draws [`Quad`]s with instanced draw calls: all quads of a frame are uploaded at once, and drawn in ranges
pub(crate) struct QuadPipeline {
	pipeline: wgpu::RenderPipeline,
	viewport: wgpu::Buffer,
	bind_group: wgpu::BindGroup,

	/// Instance buffer, grown when a frame needs more room
	instances: wgpu::Buffer,
}

impl QuadPipeline {
	pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
		let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/quad.wgsl"));

		let viewport = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("nvui-quad-viewport"),
			size: size_of::<[f32; 4]>() as u64,
			usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			label: Some("nvui-quad-bind-group-layout"),
			entries: &[wgpu::BindGroupLayoutEntry {
				binding: 0,
				visibility: wgpu::ShaderStages::VERTEX,
				ty: wgpu::BindingType::Buffer {
					ty: wgpu::BufferBindingType::Uniform,
					has_dynamic_offset: false,
					min_binding_size: None,
				},
				count: None,
			}],
		});

		let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			label: Some("nvui-quad-bind-group"),
			layout: &bind_group_layout,
			entries: &[wgpu::BindGroupEntry { binding: 0, resource: viewport.as_entire_binding() }],
		});

		let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
			label: Some("nvui-quad-pipeline-layout"),
			bind_group_layouts: &[Some(&bind_group_layout)],
			..Default::default()
		});

		let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("nvui-quad-pipeline"),
			layout: Some(&layout),
			vertex: wgpu::VertexState {
				module: &shader,
				entry_point: Some("vs_main"),
				buffers: &[Some(wgpu::VertexBufferLayout {
					array_stride: size_of::<Quad>() as u64,
					step_mode: wgpu::VertexStepMode::Instance,
					attributes: &wgpu::vertex_attr_array![
						0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x2, 4 => Float32x4
					],
				})],
				compilation_options: wgpu::PipelineCompilationOptions::default(),
			},
			fragment: Some(wgpu::FragmentState {
				module: &shader,
				entry_point: Some("fs_main"),
				targets: &[Some(wgpu::ColorTargetState {
					format,
					blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
					write_mask: wgpu::ColorWrites::ALL,
				})],
				compilation_options: wgpu::PipelineCompilationOptions::default(),
			}),
			primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
			depth_stencil: None,
			multisample: wgpu::MultisampleState::default(),
			cache: None,
			multiview_mask: None,
		});

		let instances = instance_buffer(device, 0);
		Self { pipeline, viewport, bind_group, instances }
	}

	/// Uploads `quads` for a surface of `width` × `height` pixels.
	pub(crate) fn prepare(
		&mut self,
		device: &wgpu::Device,
		queue: &wgpu::Queue,
		width: u32,
		height: u32,
		quads: &[Quad],
	) {
		queue.write_buffer(&self.viewport, 0, bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]));

		let size = size_of_val(quads) as u64;
		if size > self.instances.size() {
			self.instances = instance_buffer(device, size.next_power_of_two());
		}
		queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(quads));
	}

	/// Draws the quads at `range` of those last prepared.
	pub(crate) fn render(&self, pass: &mut wgpu::RenderPass<'_>, range: Range<u32>) {
		if range.is_empty() {
			return;
		}
		pass.set_pipeline(&self.pipeline);
		pass.set_bind_group(0, &self.bind_group, &[]);
		pass.set_vertex_buffer(0, self.instances.slice(..));
		pass.draw(0..4, range);
	}
}

fn instance_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
	device.create_buffer(&wgpu::BufferDescriptor {
		label: Some("nvui-quad-instances"),
		size,
		usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
		mapped_at_creation: false,
	})
}

/// Adds a quad for each run of cells in `layer` with the same background. Default backgrounds are drawn too, so a
/// layer covers whatever is below it.
pub(crate) fn cell_backgrounds(quads: &mut Vec<Quad>, layer: &CellLayer, cell: CellSize, srgb_surface: bool) {
	let (width, height) = (cell.width as f32, cell.height as f32);

	for (row, cells) in (layer.row..).zip(layer.cells.chunks_exact((layer.width as usize).max(1))) {
		for (col, len, color, blend) in background_runs(cells) {
			let rect = [(layer.col + col) as f32 * width, row as f32 * height, len as f32 * width, height];
			let [r, g, b, _] = rgba(color, srgb_surface);
			// What's under it shows through by `blend`, like in Neovim's own compositor, where the cell is blank.
			let opacity = 1.0 - f32::from(blend.min(100)) / 100.0;
			if opacity > 0.0 {
				quads.push(Quad::fill(rect, [r, g, b, opacity]));
			}
		}
	}
}

/// Adds quads for the underlines, strikethroughs and overlines in `layer`: one per line and run of neighbouring cells
/// with the same decoration and text color. Underlines break around the text's ink where `ink` (by row) says it
/// crosses them, given as x ranges in pixels from the row's left.
pub(crate) fn cell_decorations<'a>(
	quads: &mut Vec<Quad>,
	layer: &CellLayer,
	cell: CellSize,
	lines: LineMetrics,
	srgb_surface: bool,
	ink: impl Fn(usize) -> &'a [Range<f32>],
) {
	let rows = layer.cells.chunks_exact((layer.width as usize).max(1));
	for (index, (row, cells)) in (layer.row..).zip(rows).enumerate() {
		// In surface pixels, like the quads
		let left = (layer.col * cell.width) as f32;
		let gaps: Vec<Range<f32>> = ink(index).iter().map(|gap| gap.start + left..gap.end + left).collect();

		let mut col = layer.col;
		for run in cells.chunk_by(|a, b| (a.decoration, a.style.foreground) == (b.decoration, b.style.foreground)) {
			let rect = [(col * cell.width) as f32, (row * cell.height) as f32, (run.len() as u32 * cell.width) as f32];
			decoration_quads(quads, &run[0], rect, cell, lines, &gaps, srgb_surface);
			col += run.len() as u32;
		}
	}
}

/// Where `underline` goes in a cell: its top and bottom, in pixels from the cell's top
pub(crate) fn underline_band(underline: Underline, cell: CellSize, lines: LineMetrics) -> (f32, f32) {
	let (cell_height, thickness, y) = (cell.height as f32, lines.thickness, lines.underline_top);
	match underline {
		Underline::Single | Underline::Dotted | Underline::Dashed => (y, y + thickness),
		// Two lines with a gap as thick as a line, moved up if they would leave the cell
		Underline::Double => {
			let y = y.min(cell_height - 3.0 * thickness).max(0.0);
			(y, y + 3.0 * thickness)
		},
		// A wave around the underline, as high as the line metrics say
		Underline::Curly => {
			let height = lines.wave_height.min(cell_height);
			let y = (y + (thickness - height) / 2.0).clamp(0.0, cell_height - height);
			(y, y + height)
		},
	}
}

/// Quads for the decoration of `cell`, drawn across a run of cells at `left`, `top`, `width` pixels wide. The
/// underline breaks at `gaps`, x ranges in surface pixels.
fn decoration_quads(
	quads: &mut Vec<Quad>,
	cell: &RenderCell,
	[left, top, width]: [f32; 3],
	size: CellSize,
	lines: LineMetrics,
	gaps: &[Range<f32>],
	srgb_surface: bool,
) {
	let Decoration { underline, strikethrough, overline, underline_color } = cell.decoration;
	let (cell_width, thickness) = (size.width as f32, lines.thickness);
	let line = |y: f32, height: f32| [left, top + y, width, height];

	if let Some(underline) = underline {
		let color = rgba(underline_color, srgb_surface);
		let (y, bottom) = underline_band(underline, size, lines);
		let start = quads.len();
		match underline {
			Underline::Single => quads.push(Quad::fill(line(y, thickness), color)),
			Underline::Double => {
				quads.push(Quad::fill(line(y, thickness), color));
				quads.push(Quad::fill(line(y + 2.0 * thickness, thickness), color));
			},
			// One period per cell
			Underline::Curly => quads.push(Quad::patterned(line(y, bottom - y), color, WAVE, cell_width, thickness)),
			Underline::Dotted => {
				quads.push(Quad::patterned(line(y, thickness), color, DOTS, 2.0 * thickness, thickness));
			},
			Underline::Dashed => {
				quads.push(Quad::patterned(line(y, thickness), color, DASHES, 6.0 * thickness, thickness));
			},
		}
		cut(quads, start, gaps);
	}

	let text_color = rgba(cell.style.foreground, srgb_surface);
	if strikethrough {
		quads.push(Quad::fill(line(lines.strikeout_top, lines.strikeout_thickness), text_color));
	}
	if overline {
		quads.push(Quad::fill(line(0.0, thickness), text_color));
	}
}

/// Cuts the quads from `start` on around `gaps`, sorted x ranges in surface pixels. Patterns follow the surface x, so
/// a wave or dashes continue across a gap.
fn cut(quads: &mut Vec<Quad>, start: usize, gaps: &[Range<f32>]) {
	if gaps.is_empty() {
		return;
	}
	let pieces: Vec<Quad> = quads
		.drain(start..)
		.flat_map(|quad| {
			let [left, top, width, height] = quad.rect;
			let right = left + width;
			let mut x = left;
			let mut pieces = Vec::new();
			for gap in gaps.iter().filter(|gap| gap.end > left && gap.start < right) {
				if gap.start > x {
					pieces.push(Quad { rect: [x, top, gap.start - x, height], ..quad });
				}
				x = x.max(gap.end);
			}
			if x < right {
				pieces.push(Quad { rect: [x, top, right - x, height], ..quad });
			}
			pieces
		})
		.collect();
	quads.extend(pieces);
}

/// Adds a quad for the cursor of `layer`, if it's a bar. A block cursor is drawn as the cells under it (see
/// `cursor_piece` in the renderer).
pub(crate) fn bar_cursor(quads: &mut Vec<Quad>, layer: &CellLayer, cell: CellSize, srgb_surface: bool) {
	let Some(cursor @ Cursor { shape: CursorShape::Bar { orientation, percentage }, .. }) = layer.cursor else {
		return;
	};

	let (left, top) = ((layer.col + cursor.col) * cell.width, (layer.row + cursor.row) * cell.height);
	let thickness = |size: u32| (size * percentage).div_ceil(100).clamp(1, size);
	let rect = match orientation {
		BarOrientation::Vertical => [left, top, thickness(cell.width), cell.height],
		BarOrientation::Horizontal => {
			let height = thickness(cell.height);
			[left, top + cell.height - height, cursor.width * cell.width, height]
		},
	};
	quads.push(Quad::fill(rect.map(|value| value as f32), rgba(cursor.color, srgb_surface)));
}

/// Adds the quads of `shadow`, with cells of `cell`. Its area is first cut into rectangles that don't overlap: a blur
/// is linear, so the shadow of their union is the sum of theirs, and where the area's rectangles overlap (a window and
/// its border, say) the shadow isn't drawn twice. Blending the pieces instead of summing them differs by well under 1%.
pub(crate) fn area_shadow(quads: &mut Vec<Quad>, shadow: &LayerShadow, cell: CellSize, srgb_surface: bool) {
	for [col, row, width, height] in disjoint(&shadow.area) {
		let rect = [col * cell.width, row * cell.height, width * cell.width, height * cell.height];
		quads.push(Quad::shadow(rect.map(|value| value as f32), shadow.shadow, srgb_surface));
	}
}

/// The area of `rects` (left, top, width, height) as rectangles that don't overlap: by bands between the rectangles'
/// top and bottom edges, each band in runs of covered columns
fn disjoint(rects: &[[u32; 4]]) -> Vec<[u32; 4]> {
	let edges = |start: usize, size: usize| {
		let mut edges: Vec<u32> = rects.iter().flat_map(|rect| [rect[start], rect[start] + rect[size]]).collect();
		edges.sort_unstable();
		edges.dedup();
		edges
	};
	let (xs, ys) = (edges(0, 2), edges(1, 3));
	let covers = |rect: &[u32; 4], x: u32, y: u32| {
		(rect[0]..rect[0] + rect[2]).contains(&x) && (rect[1]..rect[1] + rect[3]).contains(&y)
	};

	let mut pieces = Vec::new();
	for band in ys.windows(2) {
		let (top, bottom) = (band[0], band[1]);
		let mut run: Option<u32> = None;
		for column in xs.windows(2) {
			let covered = rects.iter().any(|rect| covers(rect, column[0], top));
			match (covered, run) {
				(true, None) => run = Some(column[0]),
				(false, Some(left)) => {
					pieces.push([left, top, column[0] - left, bottom - top]);
					run = None;
				},
				_ => (),
			}
		}
		if let (Some(left), Some(&right)) = (run, xs.last()) {
			pieces.push([left, top, right - left, bottom - top]);
		}
	}
	pieces
}

/// Runs of neighbouring cells with the same background and blend, as (first column, length, color, blend)
fn background_runs(cells: &[RenderCell]) -> impl Iterator<Item = (u32, u32, u32, u8)> {
	let same = |a: &RenderCell, b: &RenderCell| (a.background, a.blend) == (b.background, b.blend);
	cells.chunk_by(same).scan(0, |col, run| {
		let start = *col;
		*col += run.len() as u32;
		Some((start, run.len() as u32, run[0].background, run[0].blend))
	})
}

#[cfg(test)]
mod tests {
	use arrayvec::ArrayString;

	use super::{DASHES, DOTS, Quad, QuadPipeline, WAVE, background_runs, bar_cursor, cell_decorations, disjoint};
	use crate::font::LineMetrics;
	use crate::{
		BarOrientation, CellLayer, CellSize, Cursor, CursorShape, Decoration, RenderCell, TextStyle, Underline,
	};

	fn cells(backgrounds: &[u32]) -> Vec<RenderCell> {
		let style = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
		backgrounds
			.iter()
			.map(|&background| RenderCell {
				text: ArrayString::from("x").unwrap(),
				style,
				background,
				blend: 0,
				decoration: Decoration::default(),
			})
			.collect()
	}

	#[test]
	fn runs_merge_equal_backgrounds() {
		let row = cells(&[1, 1, 2, 2, 2, 1]);
		assert_eq!(background_runs(&row).collect::<Vec<_>>(), [(0, 2, 1, 0), (2, 3, 2, 0), (5, 1, 1, 0)]);
	}

	#[test]
	fn runs_split_where_the_blend_changes() {
		let mut row = cells(&[1, 1, 1]);
		row[2].blend = 40;
		assert_eq!(background_runs(&row).collect::<Vec<_>>(), [(0, 2, 1, 0), (2, 1, 1, 40)]);
	}

	#[test]
	fn runs_of_a_plain_row() {
		assert_eq!(background_runs(&cells(&[7; 4])).collect::<Vec<_>>(), [(0, 4, 7, 0)]);
		assert_eq!(background_runs(&[]).count(), 0);
	}

	#[test]
	fn double_width_halves_keep_their_backgrounds() {
		let mut row = cells(&[1, 2, 2, 1]);
		row[2].text = ArrayString::new();
		assert_eq!(background_runs(&row).collect::<Vec<_>>(), [(0, 1, 1, 0), (1, 2, 2, 0), (3, 1, 1, 0)]);
	}

	/// Rectangles of a bar cursor at (row 2, col 3) of a layer placed at (row 1, col 1), with 8×19 px cells
	fn bar_rect(orientation: BarOrientation, width: u32, percentage: u32) -> [f32; 4] {
		let shape = CursorShape::Bar { orientation, percentage };
		let cursor = Cursor { row: 2, col: 3, width, shape, color: 0xFFFFFF, text_color: 0, animate: true };
		let layer = CellLayer {
			id: 1,
			row: 1,
			col: 1,
			width: 10,
			height: 5,
			cells: Vec::new(),
			cursor: Some(cursor),
			scroll: None,
			shadow: None,
		};
		let mut quads = Vec::new();
		bar_cursor(&mut quads, &layer, CellSize { width: 8, height: 19 }, false);
		quads[0].rect
	}

	/// Decoration quads for a run of 3 cells with `decoration` at (row 1, col 2), with 8×19 px cells and the lines of
	/// JetBrains Mono at 14 px
	fn decorations(decoration: Decoration) -> Vec<Quad> {
		let style = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
		let blank = RenderCell {
			text: ArrayString::from(" ").unwrap(),
			style,
			background: 0,
			blend: 0,
			decoration: Decoration::default(),
		};
		let mut cells = vec![blank; 6];
		for cell in &mut cells[2..5] {
			cell.decoration = decoration;
		}
		let layer =
			CellLayer { id: 1, row: 1, col: 0, width: 6, height: 1, cells, cursor: None, scroll: None, shadow: None };
		let lines = LineMetrics {
			underline_top: 17.0,
			thickness: 1.0,
			wave_height: 4.0,
			ink_gaps: None,
			strikeout_top: 10.0,
			strikeout_thickness: 1.0,
		};
		let mut quads = Vec::new();
		cell_decorations(&mut quads, &layer, CellSize { width: 8, height: 19 }, lines, false, |_| &[]);
		quads
	}

	const GREEN: u32 = 0x00FF00;

	fn underline(style: Underline) -> Decoration {
		Decoration { underline: Some(style), underline_color: GREEN, ..Decoration::default() }
	}

	#[test]
	fn decoration_rects() {
		// The run starts at (2 * 8, 1 * 19) = (16, 19) and is 24 px wide.
		let single = decorations(underline(Underline::Single));
		assert_eq!(single, [Quad::fill([16.0, 36.0, 24.0, 1.0], [0.0, 1.0, 0.0, 1.0])]);

		// Moved up so both lines fit in the 19 px cell: 16 + 3 > 19 would not
		let double: Vec<_> = decorations(underline(Underline::Double)).iter().map(|quad| quad.rect).collect();
		assert_eq!(double, [[16.0, 35.0, 24.0, 1.0], [16.0, 37.0, 24.0, 1.0]]);

		// A wave 4 px high around the underline (15.5 to 19.5), moved up to stay in the cell, one period per cell
		let curly = decorations(underline(Underline::Curly))[0];
		assert_eq!((curly.rect, curly.pattern), ([16.0, 34.0, 24.0, 4.0], [WAVE, 8.0, 1.0, 0.0]));

		assert_eq!(decorations(underline(Underline::Dotted))[0].pattern, [DOTS, 2.0, 1.0, 0.0]);
		assert_eq!(decorations(underline(Underline::Dashed))[0].pattern, [DASHES, 6.0, 1.0, 0.0]);
	}

	#[test]
	fn strikethrough_and_overline_use_the_text_color() {
		let quads = decorations(Decoration {
			strikethrough: true,
			overline: true,
			underline_color: GREEN,
			..Decoration::default()
		});
		let white = [1.0, 1.0, 1.0, 1.0];
		assert_eq!(quads, [Quad::fill([16.0, 29.0, 24.0, 1.0], white), Quad::fill([16.0, 19.0, 24.0, 1.0], white)]);
	}

	#[test]
	fn undecorated_cells_have_no_quads() {
		assert!(decorations(Decoration::default()).is_empty());
		// An underline color alone draws nothing.
		assert!(decorations(Decoration { underline_color: GREEN, ..Decoration::default() }).is_empty());
	}

	#[test]
	fn shadow_areas_are_cut_into_rectangles_that_dont_overlap() {
		// Side by side, like a picker's windows: one rectangle
		assert_eq!(disjoint(&[[0, 0, 4, 2], [4, 0, 3, 2]]), [[0, 0, 7, 2]]);
		// A window inside its border: the border's rectangle, by bands
		assert_eq!(disjoint(&[[0, 0, 10, 5], [1, 1, 8, 3]]), [[0, 0, 10, 1], [0, 1, 10, 3], [0, 4, 10, 1]]);
		// A menu with a shorter window beside it
		assert_eq!(disjoint(&[[0, 0, 4, 4], [4, 0, 3, 2]]), [[0, 0, 7, 2], [0, 2, 4, 2]]);
		// Apart
		assert_eq!(disjoint(&[[0, 0, 2, 1], [5, 0, 2, 1]]), [[0, 0, 2, 1], [5, 0, 2, 1]]);
		assert_eq!(disjoint(&[]), Vec::<[u32; 4]>::new());
	}

	#[test]
	fn bar_cursor_rects() {
		// The cell starts at (4 * 8, 3 * 19) = (32, 57).
		assert_eq!(bar_rect(BarOrientation::Vertical, 1, 25), [32.0, 57.0, 2.0, 19.0]);
		// 20% of 19 px is 3.8, rounded up; the bar sits on the bottom edge and spans both halves of a wide character.
		assert_eq!(bar_rect(BarOrientation::Horizontal, 2, 20), [32.0, 72.0, 16.0, 4.0]);
		// At least 1 px, at most the whole cell
		assert_eq!(bar_rect(BarOrientation::Vertical, 1, 0), [32.0, 57.0, 1.0, 19.0]);
		assert_eq!(bar_rect(BarOrientation::Horizontal, 1, 150), [32.0, 57.0, 8.0, 19.0]);
	}

	const SIZE: u32 = 64;

	/// Draws `quads` on black into a `SIZE`×`SIZE` texture and returns its pixels row by row, or `None` without a GPU
	fn draw(quads: &[Quad]) -> Option<Vec<[u8; 4]>> {
		smol::block_on(async {
			let gpu = crate::test_gpu::gpu()?;
			let (device, queue) = (&gpu.device, &gpu.queue);

			let format = wgpu::TextureFormat::Rgba8Unorm;
			let extent = wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 };
			let texture = device.create_texture(&wgpu::TextureDescriptor {
				label: None,
				size: extent,
				mip_level_count: 1,
				sample_count: 1,
				dimension: wgpu::TextureDimension::D2,
				format,
				usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
				view_formats: &[],
			});

			let mut pipeline = QuadPipeline::new(device, format);
			pipeline.prepare(device, queue, SIZE, SIZE, quads);

			let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
			let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
			{
				let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
					label: None,
					color_attachments: &[Some(wgpu::RenderPassColorAttachment {
						view: &view,
						resolve_target: None,
						depth_slice: None,
						ops: wgpu::Operations {
							load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
							store: wgpu::StoreOp::Store,
						},
					})],
					depth_stencil_attachment: None,
					occlusion_query_set: None,
					timestamp_writes: None,
					multiview_mask: None,
				});
				pipeline.render(&mut pass, 0..quads.len() as u32);
			}

			let buffer = device.create_buffer(&wgpu::BufferDescriptor {
				label: None,
				size: u64::from(SIZE * SIZE * 4),
				usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
				mapped_at_creation: false,
			});
			let layout = wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIZE * 4), rows_per_image: None };
			encoder.copy_texture_to_buffer(
				texture.as_image_copy(),
				wgpu::TexelCopyBufferInfo { buffer: &buffer, layout },
				extent,
			);
			queue.submit(Some(encoder.finish()));

			buffer.map_async(wgpu::MapMode::Read, .., |result| result.unwrap());
			device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
			let pixels = buffer.get_mapped_range(..).unwrap().as_chunks::<4>().0.to_vec();
			Some(pixels)
		})
	}

	fn pixel(pixels: &[[u8; 4]], x: u32, y: u32) -> [u8; 4] {
		pixels[(y * SIZE + x) as usize]
	}

	const BLACK: [u8; 4] = [0, 0, 0, 255];
	const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

	#[test]
	fn gpu_square_edges_are_sharp() {
		let Some(pixels) = draw(&[Quad::fill([8.0, 8.0, 16.0, 16.0], RED)]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		// Covers pixels 8..24 exactly, without partly covered pixels at the edges
		for (x, y) in [(8, 8), (23, 8), (8, 23), (23, 23), (16, 16)] {
			assert_eq!(pixel(&pixels, x, y), [255, 0, 0, 255], "inside at ({x}, {y})");
		}
		for (x, y) in [(7, 8), (24, 8), (8, 7), (8, 24), (40, 40)] {
			assert_eq!(pixel(&pixels, x, y), BLACK, "outside at ({x}, {y})");
		}
	}

	#[test]
	fn gpu_adjacent_quads_leave_no_seam() {
		let quads = [Quad::fill([0.0, 0.0, 10.0, 4.0], RED), Quad::fill([10.0, 0.0, 10.0, 4.0], RED)];
		let Some(pixels) = draw(&quads) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		assert!((0..20).all(|x| pixel(&pixels, x, 2) == [255, 0, 0, 255]));
	}

	#[test]
	fn gpu_rounded_corners_are_anti_aliased() {
		let quad = Quad { corner_radius: 12.0, ..Quad::fill([32.0, 0.0, 32.0, 32.0], [0.0, 1.0, 0.0, 1.0]) };
		let Some(pixels) = draw(&[quad]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		assert_eq!(pixel(&pixels, 48, 16), [0, 255, 0, 255], "center");
		assert_eq!(pixel(&pixels, 32, 0), BLACK, "outside the rounded corner");
		// Pixel (35, 3) has its center about 12.02 px from the corner circle's center, so about half covered.
		let [r, g, b, _] = pixel(&pixels, 35, 3);
		assert!((r, b) == (0, 0) && (100..156).contains(&g), "edge pixel: {g}");
	}

	#[test]
	fn gpu_dots_and_dashes() {
		let dots = Quad::patterned([0.0, 0.0, 12.0, 1.0], RED, DOTS, 2.0, 1.0);
		let dashes = Quad::patterned([0.0, 2.0, 12.0, 1.0], RED, DASHES, 6.0, 1.0);
		let Some(pixels) = draw(&[dots, dashes]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		let row = |y| (0..12).map(|x| pixel(&pixels, x, y)[0] > 128).collect::<Vec<_>>();
		let (on, off) = (true, false);
		assert_eq!(row(0), [on, off, on, off, on, off, on, off, on, off, on, off]);
		assert_eq!(row(2), [on, on, on, off, off, off, on, on, on, off, off, off]);
	}

	#[test]
	fn gpu_wave_swings_around_its_center() {
		// 8 px high, one period over 16 px: up in the first half, down in the second
		let Some(pixels) = draw(&[Quad::patterned([0.0, 0.0, 16.0, 8.0], RED, WAVE, 16.0, 1.0)]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		let drawn_rows = |x| (0..8).filter(|&y| pixel(&pixels, x, y)[0] > 64).collect::<Vec<_>>();
		// Through the middle at the start, lowest on screen at a quarter period, highest at three quarters
		assert!(drawn_rows(0).iter().all(|&y| (3..=5).contains(&y)), "x = 0: {:?}", drawn_rows(0));
		assert!(drawn_rows(4).iter().all(|&y| y >= 6), "x = 4: {:?}", drawn_rows(4));
		assert!(drawn_rows(12).iter().all(|&y| y <= 1), "x = 12: {:?}", drawn_rows(12));
		assert!((0..16).all(|x| !drawn_rows(x).is_empty()), "the wave has no gaps");
	}

	#[test]
	fn gpu_border() {
		let quad = Quad {
			border_color: [1.0, 1.0, 1.0, 1.0],
			border_width: 4.0,
			..Quad::fill([0.0, 32.0, 32.0, 32.0], [0.0, 0.0, 1.0, 1.0])
		};
		let Some(pixels) = draw(&[quad]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		assert_eq!(pixel(&pixels, 1, 33), [255, 255, 255, 255], "border");
		assert_eq!(pixel(&pixels, 3, 48), [255, 255, 255, 255], "border, inner edge");
		assert_eq!(pixel(&pixels, 4, 48), [0, 0, 255, 255], "fill, next to the border");
		assert_eq!(pixel(&pixels, 16, 48), [0, 0, 255, 255], "fill");
	}

	#[test]
	fn cut_around_gaps() {
		let line = Quad::fill([0.0, 10.0, 80.0, 1.0], [1.0; 4]);
		let mut quads = vec![line, line];
		// Only quads from index 1 on are cut; gaps may reach past the quad.
		super::cut(&mut quads, 1, &[-5.0..2.0, 10.0..20.0, 50.0..60.0, 79.0..90.0]);
		let pieces: Vec<_> = quads[1..].iter().map(|quad| (quad.rect[0], quad.rect[2])).collect();
		assert_eq!(pieces, [(2.0, 8.0), (20.0, 30.0), (60.0, 19.0)]);
		assert_eq!(quads[0], line);
	}
}
