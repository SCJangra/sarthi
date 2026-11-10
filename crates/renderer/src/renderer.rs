use std::borrow::Cow;
use std::ops::Range;
use std::time::Instant;

use crate::color::{rgba, wgpu_color};
use crate::cursor::{CursorAnimation, cursor_rect};
use crate::font::LineMetrics;
use crate::quad::{Quad, QuadPipeline, area_shadow, bar_cursor, cell_backgrounds, cell_decorations};
use crate::scroll::ScrollAnimations;
use crate::shape::ShapePipeline;
use crate::text::TextPipeline;
use crate::{
	CellLayer, CellSize, Cursor, CursorMotion, CursorShape, Fonts, Layer, Motion, RenderCell, RendererConfig,
	RendererError, TextSize, TextStyle,
};

pub struct Renderer {
	surface: wgpu::Surface<'static>,
	device: wgpu::Device,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	clear_color: wgpu::Color,
	quads: QuadPipeline,
	text: TextPipeline,
	shape: ShapePipeline,

	/// What to draw, the text sliding in it after scrolling, and the cursor on its way to where it is
	layers: Vec<Layer>,
	scrolls: ScrollAnimations,
	cursor: CursorAnimation,

	/// Quads of the current frame, layer by layer, and each layer's ranges of them; kept to reuse the allocations
	frame_quads: Vec<Quad>,
	layer_quads: Vec<LayerQuads>,
}

impl Renderer {
	/// Creates a renderer that draws to `target`.
	///
	/// The surface takes ownership of `target` (e.g. an `Arc<Window>`), which keeps the window
	/// alive for as long as the renderer exists.
	pub async fn new(
		target: impl Into<wgpu::SurfaceTarget<'static>>,
		width: u32,
		height: u32,
	) -> Result<Self, RendererError> {
		Self::with_config(target, width, height, RendererConfig::default()).await
	}

	pub async fn with_config(
		target: impl Into<wgpu::SurfaceTarget<'static>>,
		width: u32,
		height: u32,
		renderer_config: RendererConfig,
	) -> Result<Self, RendererError> {
		let instance = wgpu::Instance::default();
		let surface = instance.create_surface(target)?;

		let adapter = instance
			.request_adapter(&wgpu::RequestAdapterOptions {
				power_preference: renderer_config.power_preference,
				compatible_surface: Some(&surface),
				force_fallback_adapter: false,
				// Only needed when exposing wgpu to untrusted content, e.g. in a web browser.
				apply_limit_buckets: false,
			})
			.await
			.map_err(|_| RendererError::NoAdapter)?;

		let (device, queue) = adapter
			.request_device(&wgpu::DeviceDescriptor {
				label: Some("nvui-renderer-device"),
				required_features: wgpu::Features::empty(),
				required_limits: wgpu::Limits::default(),
				memory_hints: wgpu::MemoryHints::Performance,
				trace: wgpu::Trace::Off,
				experimental_features: wgpu::ExperimentalFeatures::disabled(),
			})
			.await?;

		let capabilities = surface.get_capabilities(&adapter);

		let format = if renderer_config.prefer_srgb_surface_format {
			capabilities
				.formats
				.iter()
				.copied()
				.find(wgpu::TextureFormat::is_srgb)
				.or_else(|| capabilities.formats.first().copied())
				.ok_or(RendererError::NoSurfaceFormat)?
		} else {
			capabilities.formats.first().copied().ok_or(RendererError::NoSurfaceFormat)?
		};

		let present_mode = match renderer_config.present_mode {
			Some(mode) => {
				if capabilities.present_modes.contains(&mode) {
					mode
				} else {
					return Err(RendererError::UnsupportedPresentMode(mode));
				}
			},
			None => capabilities
				.present_modes
				.first()
				.copied()
				.ok_or(RendererError::NoPresentMode)?,
		};

		let alpha_mode = capabilities.alpha_modes.first().copied().ok_or(RendererError::NoAlphaMode)?;

		let config = wgpu::SurfaceConfiguration {
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
			format,
			width: width.max(1),
			height: height.max(1),
			present_mode,
			alpha_mode,
			view_formats: vec![],
			color_space: wgpu::SurfaceColorSpace::Auto,
			desired_maximum_frame_latency: renderer_config.desired_maximum_frame_latency.max(1),
		};

		surface.configure(&device, &config);

		let clear_color = wgpu_color(renderer_config.background, config.format.is_srgb());

		let quads = QuadPipeline::new(&device, config.format);
		let text = TextPipeline::new(&device, &queue, config.format);
		let shape = ShapePipeline::new(&device, config.format);

		Ok(Self {
			surface,
			device,
			queue,
			config,
			clear_color,
			quads,
			text,
			shape,
			layers: Vec::new(),
			scrolls: ScrollAnimations::new(renderer_config.scroll),
			cursor: CursorAnimation::new(renderer_config.cursor),
			frame_quads: Vec::new(),
			layer_quads: Vec::new(),
		})
	}

	pub fn resize(&mut self, width: u32, height: u32) {
		self.config.width = width.max(1);
		self.config.height = height.max(1);
		self.surface.configure(&self.device, &self.config);
	}

	/// Sets the color the window is cleared to, as `0xRRGGBB` in sRGB (the format Neovim uses)
	pub fn set_clear_color(&mut self, rgb: u32) {
		self.clear_color = wgpu_color(rgb, self.config.format.is_srgb());
	}

	/// Shows `layers` from the next frame on, drawn in order so later ones cover earlier ones. Text that scrolled
	/// since the layers shown until now slides to its new place.
	pub fn set_layers(&mut self, layers: Vec<Layer>) {
		self.scrolls.update(&self.layers, &layers, Instant::now());
		self.layers = layers;
	}

	/// Sets how text that scrolls moves to its new place, from the next scroll on
	pub fn set_scroll_motion(&mut self, motion: Motion) {
		self.scrolls.set_motion(motion);
	}

	/// Sets how the cursor moves to a new place, from its next move on
	pub fn set_cursor_motion(&mut self, motion: CursorMotion) {
		self.cursor.set_motion(motion);
	}

	/// The layers shown, as last set
	pub fn layers(&self) -> &[Layer] {
		&self.layers
	}

	/// Whether text is still sliding or the cursor moving, so the window should be drawn again
	pub fn is_animating(&self) -> bool {
		let now = Instant::now();
		self.scrolls.is_animating(now) || self.cursor.is_animating(now)
	}

	/// Draws the layers on the clear color, with text of `size`. Laying out text needs `fonts`, which all renderers
	/// share.
	///
	/// `pre_present` runs right before the frame is handed to the window system, and only if it is, e.g. for
	/// winit's `Window::pre_present_notify`.
	pub fn render(
		&mut self,
		fonts: &mut Fonts,
		size: TextSize,
		pre_present: impl FnOnce(),
	) -> Result<Frame, RendererError> {
		let (frame, suboptimal) = match self.surface.get_current_texture() {
			wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
			wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
			wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
				return Ok(Frame::Skipped);
			},
			wgpu::CurrentSurfaceTexture::Outdated => {
				self.surface.configure(&self.device, &self.config);
				return Ok(Frame::Reconfigured);
			},
			wgpu::CurrentSurfaceTexture::Lost => return Err(RendererError::SurfaceLost),
			wgpu::CurrentSurfaceTexture::Validation => return Err(RendererError::SurfaceValidation),
		};

		let (cell, lines, srgb) = (fonts.cell_size(size), fonts.line_metrics(size), self.config.format.is_srgb());
		let (surface, now) = ((self.config.width, self.config.height), Instant::now());
		// On its way, the cursor is drawn as its shape, with a block's character only where the shape covers its cell.
		let moving = self.move_cursor(cell, now);
		let mut pieces = pieces(&self.layers, &self.scrolls, cell, surface, now);
		if let Some((corners, cursor)) = moving {
			if let Some(piece) = pieces.last_mut().filter(|piece| piece.kind == PieceKind::Cursor) {
				piece.clip = covered(piece, corners, cell);
			}
			self.shape
				.prepare(&self.queue, surface.0, surface.1, corners, rgba(cursor.color, srgb));
		}

		let resolution = glyphon::Resolution { width: self.config.width, height: self.config.height };
		self.text.prepare(&self.device, &self.queue, resolution, fonts, &pieces, size)?;

		self.frame_quads.clear();
		self.layer_quads.clear();
		for piece in &pieces {
			let moving = moving.is_some() && piece.kind == PieceKind::Cursor;
			self.layer_quads
				.push(layer_quads(&mut self.frame_quads, piece, &self.text, cell, lines, srgb, moving));
		}
		self.quads
			.prepare(&self.device, &self.queue, self.config.width, self.config.height, &self.frame_quads);

		let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

		let mut encoder = self
			.device
			.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("nvui-renderer-encoder") });

		{
			let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
				label: Some("nvui-renderer-pass"),
				color_attachments: &[Some(wgpu::RenderPassColorAttachment {
					view: &view,
					resolve_target: None,
					depth_slice: None,
					ops: wgpu::Operations { load: wgpu::LoadOp::Clear(self.clear_color), store: wgpu::StoreOp::Store },
				})],
				depth_stencil_attachment: None,
				occlusion_query_set: None,
				timestamp_writes: None,
				multiview_mask: None,
			});
			draw_layers(&mut pass, &self.quads, &self.text, &self.shape, surface, &self.layer_quads)?;
		}

		self.queue.submit(Some(encoder.finish()));
		pre_present();
		self.queue.present(frame);
		self.text.trim();

		// The frame was still shown, but the next ones should use a surface that matches again.
		if suboptimal {
			self.surface.configure(&self.device, &self.config);
		}

		Ok(Frame::Presented)
	}
}

impl Renderer {
	/// Moves the cursor animation on to the layers' cursor, with cells of `cell`, and gives the cursor and its corners
	/// at `now` (in pixels: top-left, top-right, bottom-right, bottom-left) while it's on its way.
	fn move_cursor(&mut self, cell: CellSize, now: Instant) -> Option<([[f32; 2]; 4], Cursor)> {
		let (layer, cursor) = self
			.layers
			.iter()
			.find_map(|Layer::Cells(layer)| Some((layer, layer.cursor?)))?;
		self.cursor.update(cursor_rect(layer, cursor, cell), cursor.animate, cell, now);
		if !self.cursor.is_animating(now) {
			return None;
		}
		let corners = self.cursor.corners(now)?;
		Some((corners.map(|[x, y]| [x * cell.width as f32, y * cell.height as f32]), cursor))
	}
}

/// The part of the cursor `piece`, within its clip, that the cursor's shape covers while on its way, with `corners` in
/// pixels and cells of `cell`
fn covered(piece: &Piece, corners: [[f32; 2]; 4], cell: CellSize) -> [u32; 4] {
	let layer = &piece.layer;
	let [left, top, width, height] = piece.clip;
	let (mut low, mut high) = ([left, top], [left + width, top + height]);
	let cells = [[layer.col, layer.row], [layer.col + layer.width, layer.row + layer.height]];
	let size = [cell.width, cell.height];
	for axis in 0..2 {
		let (least, most) = corners
			.iter()
			.fold((f32::MAX, f32::MIN), |(least, most), corner| (least.min(corner[axis]), most.max(corner[axis])));
		low[axis] = low[axis].max(cells[0][axis] * size[axis]).max(least.floor().max(0.0) as u32);
		high[axis] = high[axis].min(cells[1][axis] * size[axis]).min(most.ceil().max(0.0) as u32);
	}
	[low[0], low[1], high[0].saturating_sub(low[0]), high[1].saturating_sub(low[1])]
}

/// What happened when rendering a frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
	/// The frame was drawn and shown
	Presented,

	/// Nothing could be drawn right now (e.g. the window is hidden); try again on a later redraw
	Skipped,

	/// The surface was out of date and has been reconfigured; draw again
	Reconfigured,
}

/// A cell layer as drawn in a frame: a whole layer, then over it the scrolling region of one whose text slides; and after
/// all layers, the cursor
pub(crate) struct Piece<'a> {
	pub layer: Cow<'a, CellLayer>,

	/// Which part of its layer this is, which keeps its laid-out rows apart from the others'
	pub kind: PieceKind,

	/// Pixels the layer is drawn below its row, or above if negative
	pub offset: f32,

	/// What's drawn outside this rectangle of the window is cut off: left, top, width, height in pixels
	pub clip: [u32; 4],
}

/// Which part of a layer a [`Piece`] draws
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PieceKind {
	/// The whole layer, without its cursor
	Layer,

	/// The scrolling region of a layer whose text slides
	Region,

	/// The layer's cursor, where Neovim put it: drawn over text that's still sliding into place, so it never leaves the
	/// window or crosses `scrolloff`. Drawn last, over every layer.
	Cursor,
}

/// The pieces to draw `layers` at `now`, in order, on a `surface` of width × height pixels
fn pieces<'a>(
	layers: &'a [Layer],
	scrolls: &ScrollAnimations,
	cell: CellSize,
	surface: (u32, u32),
	now: Instant,
) -> Vec<Piece<'a>> {
	// Cells to pixels, cut to the surface: a scissor rectangle can't reach outside it
	let clip = |col: u32, row: u32, width: u32, height: u32| {
		let (left, top) = ((col * cell.width).min(surface.0), (row * cell.height).min(surface.1));
		let (right, bottom) =
			(((col + width) * cell.width).min(surface.0), ((row + height) * cell.height).min(surface.1));
		[left, top, right - left, bottom - top]
	};

	let mut pieces = Vec::with_capacity(layers.len() + 1);
	let mut cursor_piece = None;
	for Layer::Cells(layer) in layers {
		let whole = clip(layer.col, layer.row, layer.width, layer.height);
		pieces.push(Piece { layer: Cow::Borrowed(layer), kind: PieceKind::Layer, offset: 0.0, clip: whole });

		if let Some(region) = scrolls.region(layer, now) {
			let clip = clip(region.layer.col, region.layer.row, region.layer.width, region.height);
			// Whole pixels keep the text sharp.
			let offset = (region.offset * cell.height as f32).round();
			pieces.push(Piece { layer: Cow::Owned(region.layer), kind: PieceKind::Region, offset, clip });
		}

		if let Some(cursor) = layer.cursor {
			let layer = Cow::Owned(cursor_layer(layer, cursor));
			cursor_piece = Some(Piece { layer, kind: PieceKind::Cursor, offset: 0.0, clip: whole });
		}
	}
	// Over everything, like in the TUI
	pieces.extend(cursor_piece);
	pieces
}

/// The cursor of `layer` as a layer of its own. A block is the cells under it, in the cursor's colors, so its
/// character stays readable; a bar is drawn over nothing.
fn cursor_layer(layer: &CellLayer, cursor: Cursor) -> CellLayer {
	let (row, col) = (layer.row + cursor.row, layer.col + cursor.col);
	let empty = CellLayer {
		row,
		col,
		width: 0,
		height: 0,
		cells: Vec::new(),
		cursor: None,
		scroll: None,
		shadow: None,
		..*layer
	};
	if cursor.shape != CursorShape::Block {
		return CellLayer { row: layer.row, col: layer.col, cursor: Some(cursor), ..empty };
	}

	let start = (cursor.row * layer.width + cursor.col) as usize;
	let Some(cells) = layer.cells.get(start..start + cursor.width as usize) else { return empty };
	let cells = cells
		.iter()
		.map(|cell| RenderCell {
			style: TextStyle { foreground: cursor.text_color, ..cell.style },
			background: cursor.color,
			..*cell
		})
		.collect();
	CellLayer { width: cursor.width, height: 1, cells, ..empty }
}

/// A layer's ranges of the frame's quads: its shadow, drawn first and uncut since it reaches past the layer; its
/// backgrounds, drawn under its text, and its decorations and bar cursor, drawn over it; and the rectangle these and the
/// text are cut to
struct LayerQuads {
	shadow: Range<u32>,
	under_text: Range<u32>,
	over_text: Range<u32>,
	clip: [u32; 4],

	/// Whether the cursor's shape is drawn first, uncut: for the cursor on its way
	shape: bool,
}

/// Adds the quads of `piece` to `quads`, with underlines breaking where `text` laid out its text over them. With `moving`,
/// `piece` is the cursor on its way, drawn over its shape.
fn layer_quads(
	quads: &mut Vec<Quad>,
	piece: &Piece,
	text: &TextPipeline,
	cell: CellSize,
	lines: LineMetrics,
	srgb: bool,
	moving: bool,
) -> LayerQuads {
	let layer = &piece.layer;
	let shadow = quads.len();
	if let Some(layer_shadow) = layer.shadow.as_ref().filter(|_| piece.kind == PieceKind::Layer) {
		area_shadow(quads, layer_shadow, cell, srgb);
	}
	let start = quads.len();
	// The shape of a cursor on its way stands in for its background or bar.
	if !moving {
		cell_backgrounds(quads, layer, cell, srgb);
	}
	let middle = quads.len();
	cell_decorations(quads, layer, cell, lines, srgb, |row| text.ink(piece, row));
	if piece.kind == PieceKind::Cursor && !moving {
		bar_cursor(quads, layer, cell, srgb);
	}
	for quad in &mut quads[start..] {
		quad.rect[1] += piece.offset;
	}
	LayerQuads {
		shadow: shadow as u32..start as u32,
		under_text: start as u32..middle as u32,
		over_text: middle as u32..quads.len() as u32,
		clip: piece.clip,
		shape: moving,
	}
}

/// Draws the layers one after another, each with its backgrounds, text, then decorations and cursor, so a later
/// layer (e.g. a floating window) covers everything of the earlier ones. A layer's shadow, and the cursor's shape drawn
/// first for the cursor on its way, aren't cut to the layer: on a `surface` of width × height pixels, they reach past it.
fn draw_layers(
	pass: &mut wgpu::RenderPass<'_>,
	quads: &QuadPipeline,
	text: &TextPipeline,
	shape: &ShapePipeline,
	surface: (u32, u32),
	layers: &[LayerQuads],
) -> Result<(), RendererError> {
	for (index, layer) in layers.iter().enumerate() {
		if !layer.shadow.is_empty() {
			pass.set_scissor_rect(0, 0, surface.0, surface.1);
			quads.render(pass, layer.shadow.clone());
		}
		if layer.shape {
			pass.set_scissor_rect(0, 0, surface.0, surface.1);
			shape.render(pass);
		}
		let [left, top, width, height] = layer.clip;
		pass.set_scissor_rect(left, top, width, height);
		quads.render(pass, layer.under_text.clone());
		text.render(pass, index)?;
		quads.render(pass, layer.over_text.clone());
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::time::Instant;

	use arrayvec::ArrayString;

	use super::{PieceKind, draw_layers, layer_quads, pieces};
	use crate::quad::QuadPipeline;
	use crate::scroll::ScrollAnimations;
	use crate::shape::ShapePipeline;
	use crate::text::TextPipeline;
	use crate::{CellLayer, Decoration, Fonts, Layer, RenderCell, RendererConfig, TextSize, TextStyle};

	/// A one-cell layer at the top-left corner showing `text` in white on `background`
	fn cell(id: u64, text: &str, background: u32) -> Layer {
		let style = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
		let cell =
			RenderCell { text: ArrayString::from(text).unwrap(), style, background, decoration: Decoration::default() };
		Layer::Cells(CellLayer {
			id,
			row: 0,
			col: 0,
			width: 1,
			height: 1,
			cells: vec![cell],
			cursor: None,
			scroll: None,
			shadow: None,
		})
	}

	/// Draws `layers` like the renderer does, into a 64×19 texture, and returns the pixel in the middle of the first cell,
	/// or `None` without a GPU
	fn middle_pixel(layers: &[Layer]) -> Option<[u8; 4]> {
		drawn_pixel(layers, None)
	}

	/// Like [`middle_pixel`], with a cursor shape over the layers if given: its corners in pixels, and its color
	fn drawn_pixel(layers: &[Layer], shape: Option<([[f32; 2]; 4], u32)>) -> Option<[u8; 4]> {
		let (pixels, cell) = drawn(layers, shape)?;
		Some(pixels[(9 * WIDTH + cell.width / 2) as usize])
	}

	/// Width of the texture the GPU tests draw to, 19 pixels (a row of cells) high
	const WIDTH: u32 = 64;

	/// Draws `layers` like the renderer does, with a cursor shape over them if given, into a [`WIDTH`] × 19 texture,
	/// and returns its pixels row by row and the cell size, or `None` without a GPU
	fn drawn(layers: &[Layer], shape: Option<([[f32; 2]; 4], u32)>) -> Option<(Vec<[u8; 4]>, crate::CellSize)> {
		let (width, height) = (WIDTH, 19);
		smol::block_on(async {
			let gpu = crate::test_gpu::gpu()?;
			let (device, queue) = (&gpu.device, &gpu.queue);
			let format = wgpu::TextureFormat::Rgba8Unorm;
			let extent = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
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

			let mut fonts = Fonts::new();
			let size = TextSize::new(14.0);
			let (cell, lines) = (fonts.cell_size(size), fonts.line_metrics(size));
			let mut text = TextPipeline::new(device, queue, format);
			let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
			let pieces = pieces(layers, &scrolls, cell, (width, height), Instant::now());
			let resolution = glyphon::Resolution { width, height };
			text.prepare(device, queue, resolution, &mut fonts, &pieces, size).unwrap();

			let mut quads = Vec::new();
			let ranges: Vec<_> = pieces
				.iter()
				.map(|piece| layer_quads(&mut quads, piece, &text, cell, lines, false, false))
				.chain(shape.map(|_| super::LayerQuads {
					shadow: 0..0,
					under_text: 0..0,
					over_text: 0..0,
					clip: [0, 0, 1, 1],
					shape: true,
				}))
				.collect();
			let shape_pipeline = ShapePipeline::new(device, format);
			if let Some((corners, color)) = shape {
				shape_pipeline.prepare(queue, width, height, corners, crate::color::rgba(color, false));
			}
			let mut pipeline = QuadPipeline::new(device, format);
			pipeline.prepare(device, queue, width, height, &quads);

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
				draw_layers(&mut pass, &pipeline, &text, &shape_pipeline, (width, height), &ranges).unwrap();
			}

			let buffer = device.create_buffer(&wgpu::BufferDescriptor {
				label: None,
				size: u64::from(width * height * 4),
				usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
				mapped_at_creation: false,
			});
			let layout =
				wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: None };
			encoder.copy_texture_to_buffer(
				texture.as_image_copy(),
				wgpu::TexelCopyBufferInfo { buffer: &buffer, layout },
				extent,
			);
			queue.submit(Some(encoder.finish()));
			buffer.map_async(wgpu::MapMode::Read, .., |result| result.unwrap());
			device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

			let pixels = buffer.get_mapped_range(..).unwrap().as_chunks::<4>().0.to_vec();
			Some((pixels, cell))
		})
	}

	#[test]
	fn gpu_later_layers_cover_earlier_ones() {
		// A full block in the lower layer, a red background in the upper one: the red covers the block's text.
		let Some(upper_on_top) = middle_pixel(&[cell(1, "█", 0x000000), cell(2, " ", 0xFF0000)]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		assert_eq!(upper_on_top, [255, 0, 0, 255]);

		// The other way around, the block covers the red.
		let block_on_top = middle_pixel(&[cell(2, " ", 0xFF0000), cell(1, "█", 0x000000)]).unwrap();
		assert_eq!(block_on_top, [255, 255, 255, 255]);
	}

	/// `layer` with a cursor of `shape` on its top-left cell, red with white text
	fn with_cursor(layer: Layer, shape: crate::CursorShape) -> Layer {
		let Layer::Cells(mut layer) = layer;
		layer.cursor = Some(crate::Cursor {
			row: 0,
			col: 0,
			width: 1,
			shape,
			color: 0xFF0000,
			text_color: 0xFFFFFF,
			animate: true,
		});
		Layer::Cells(layer)
	}

	#[test]
	fn the_cursor_is_a_piece_over_its_layer() {
		let layer = with_cursor(cell(1, "a", 0x000000), crate::CursorShape::Block);
		let cell_size = crate::CellSize { width: 8, height: 19 };
		let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
		let pieces = pieces(std::slice::from_ref(&layer), &scrolls, cell_size, (64, 19), Instant::now());

		let kinds: Vec<_> = pieces.iter().map(|piece| piece.kind).collect();
		assert_eq!(kinds, [PieceKind::Layer, PieceKind::Cursor]);
		// A block: the cell under it, in the cursor's colors, where Neovim put it
		let cursor = &pieces[1].layer;
		assert_eq!((cursor.row, cursor.col, cursor.width, cursor.height, pieces[1].offset), (0, 0, 1, 1, 0.0));
		assert_eq!((cursor.cells[0].text.as_str(), cursor.cells[0].background), ("a", 0xFF0000));
		assert_eq!(cursor.cells[0].style.foreground, 0xFFFFFF);

		// A bar: drawn as a quad, over no cells
		let bar = crate::CursorShape::Bar { orientation: crate::BarOrientation::Vertical, percentage: 25 };
		let layer = with_cursor(cell(1, "a", 0x000000), bar);
		let drawn = super::pieces(std::slice::from_ref(&layer), &scrolls, cell_size, (64, 19), Instant::now());
		assert_eq!((drawn[1].layer.cells.len(), drawn[1].layer.cursor.is_some()), (0, true));
	}

	#[test]
	fn gpu_cursor_shape_fills_between_its_corners() {
		// A slanted shape across the middle of the first cell (4, 9), over a black cell
		let slanted = [[0.0, 0.0], [6.0, 0.0], [12.0, 19.0], [2.0, 19.0]];
		let Some(pixel) = drawn_pixel(&[cell(1, " ", 0x000000)], Some((slanted, 0x00FF00))) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		assert_eq!(pixel, [0, 255, 0, 255]);

		// The same slant further right misses it.
		let right = slanted.map(|[x, y]| [x + 20.0, y]);
		assert_eq!(drawn_pixel(&[cell(1, " ", 0x000000)], Some((right, 0x00FF00))).unwrap(), [0, 0, 0, 255]);

		// Corners that briefly cross, so the shape isn't convex, still fill.
		let crossed = [[0.0, 0.0], [20.0, 0.0], [0.0, 19.0], [20.0, 19.0]];
		assert_eq!(drawn_pixel(&[cell(1, " ", 0x000000)], Some((crossed, 0x00FF00))).unwrap(), [0, 255, 0, 255]);
	}

	#[test]
	fn gpu_a_shadow_fades_out_around_its_layer() {
		// A white cell at column 3, with a red shadow, on black
		let shadow = crate::Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0xFF0000, opacity: 1.0 };
		let Layer::Cells(white) = cell(1, " ", 0xFFFFFF);
		let shadow = crate::LayerShadow { shadow, area: vec![[3, 0, 1, 1]] };
		let layer = Layer::Cells(CellLayer { col: 3, shadow: Some(shadow), ..white });
		let Some((pixels, cell)) = drawn(&[layer], None) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		let left = 3 * cell.width;
		let at = |x: u32| pixels[(9 * WIDTH + x) as usize];

		// The layer itself covers its shadow.
		assert_eq!(at(left + 1), [255, 255, 255, 255]);
		// Just outside, about half shadowed; less further out; gone past three standard deviations (12 pixels)
		let reds: Vec<u8> = [1, 4, 8].map(|distance| at(left - distance)[0]).to_vec();
		assert!((100..=140).contains(&reds[0]), "{reds:?}");
		assert!(reds[0] > reds[1] && reds[1] > reds[2] && reds[2] > 0, "{reds:?}");
		assert_eq!(at(left - 14), [0, 0, 0, 255]);
		// It's only a shadow: no green or blue in it
		assert_eq!(&at(left - 1)[1..3], &[0, 0]);
	}

	#[test]
	fn the_cursor_is_drawn_over_every_layer() {
		let lower = with_cursor(cell(1, "a", 0x000000), crate::CursorShape::Block);
		let cell_size = crate::CellSize { width: 8, height: 19 };
		let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
		let layers = [lower, cell(2, "b", 0x000000)];
		let pieces = pieces(&layers, &scrolls, cell_size, (64, 19), Instant::now());
		let kinds: Vec<_> = pieces.iter().map(|piece| (piece.layer.id, piece.kind)).collect();
		assert_eq!(kinds, [(1, PieceKind::Layer), (2, PieceKind::Layer), (1, PieceKind::Cursor)]);
	}

	#[test]
	fn a_moving_block_shows_its_character_where_its_shape_covers_it() {
		let layer = with_cursor(cell(1, "a", 0x000000), crate::CursorShape::Block);
		let cell_size = crate::CellSize { width: 8, height: 19 };
		let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
		let pieces = pieces(std::slice::from_ref(&layer), &scrolls, cell_size, (64, 19), Instant::now());
		let cursor = &pieces[1];

		// The shape reaches halfway into the cell from the right: the right half shows.
		let arriving = [[4.0, 0.0], [30.0, 0.0], [30.0, 19.0], [4.0, 19.0]];
		assert_eq!(super::covered(cursor, arriving, cell_size), [4, 0, 4, 19]);
		// Not there yet: nothing shows.
		let away = [[20.0, 0.0], [30.0, 0.0], [30.0, 19.0], [20.0, 19.0]];
		assert_eq!(super::covered(cursor, away, cell_size)[2], 0);
	}

	#[test]
	fn gpu_block_cursor_covers_its_cell() {
		// A full block in white under a red block cursor: the cursor's red covers it, and its text color is white.
		let layer = with_cursor(cell(1, " ", 0x000000), crate::CursorShape::Block);
		let Some(pixel) = middle_pixel(&[layer]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		assert_eq!(pixel, [255, 0, 0, 255]);

		let layer = with_cursor(cell(1, "█", 0x000000), crate::CursorShape::Block);
		assert_eq!(middle_pixel(&[layer]).unwrap(), [255, 255, 255, 255]);
	}

	#[test]
	fn the_cursor_stays_put_while_text_slides() {
		// A 1×3 layer that scrolled up by a row, with the cursor on its last row
		let column = |rows: [&str; 3], scroll: bool| {
			let Layer::Cells(top) = cell(1, rows[0], 0);
			let cells = rows
				.iter()
				.map(|text| RenderCell { text: ArrayString::from(text).unwrap(), ..top.cells[0] });
			let scroll = scroll.then_some(crate::Scroll { rows: 1, top: 0, bottom: 3, left: 0, right: 1 });
			let cursor = crate::Cursor {
				row: 2,
				col: 0,
				width: 1,
				shape: crate::CursorShape::Block,
				color: 0,
				text_color: 0,
				animate: true,
			};
			Layer::Cells(CellLayer { height: 3, cells: cells.collect(), cursor: Some(cursor), scroll, ..top })
		};
		let (shown, scrolled) = (column(["a", "b", "c"], false), column(["b", "c", "d"], true));
		let mut scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
		let now = Instant::now();
		scrolls.update(std::slice::from_ref(&shown), std::slice::from_ref(&scrolled), now);

		let cell_size = crate::CellSize { width: 8, height: 19 };
		let later = now + std::time::Duration::from_millis(50);
		let drawn = pieces(std::slice::from_ref(&scrolled), &scrolls, cell_size, (8, 57), later);
		let kinds: Vec<_> = drawn.iter().map(|piece| piece.kind).collect();
		assert_eq!(kinds, [PieceKind::Layer, PieceKind::Region, PieceKind::Cursor]);
		// The text is still sliding, but the cursor is on the last row, on "d", where Neovim put it.
		assert!(drawn[1].offset != 0.0, "{}", drawn[1].offset);
		assert_eq!(drawn[2].offset, 0.0);
		assert_eq!((drawn[2].layer.row, drawn[2].layer.cells[0].text.as_str()), (2, "d"));
	}
}
