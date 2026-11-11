use std::borrow::Cow;
use std::ops::Range;
use std::time::Instant;

use arrayvec::ArrayString;

use crate::color::{rgba, wgpu_color};
use crate::cursor::{CursorAnimation, cursor_rect};
use crate::font::LineMetrics;
use crate::quad::{
	FrameQuads, QuadPipeline, area_shadow, cell_backgrounds, cell_decorations, cursor_quad, outline_quad,
};
use crate::scroll::{ScrollAnimations, SlidingRegion};
use crate::shape::ShapePipeline;
use crate::text::TextPipeline;
use crate::{
	CellLayer, Cursor, CursorMotion, CursorShape, Decoration, Fonts, Layer, Motion, RenderCell, RendererConfig,
	RendererError, TextStyle,
};

pub struct Renderer {
	surface: wgpu::Surface<'static>,
	device: wgpu::Device,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	/// The window's background, `0xRRGGBB` in sRGB, and how opaque it is, from 0 to 1: what the window is cleared to
	background: u32,
	opacity: f32,
	quads: QuadPipeline,
	text: TextPipeline,
	shape: ShapePipeline,

	/// What to draw, the text sliding in it after scrolling, and the cursor on its way to where it is
	layers: Vec<Layer>,
	scrolls: ScrollAnimations,
	cursor: CursorAnimation,

	/// Quads of the current frame, layer by layer, and each layer's ranges of them; kept to reuse the allocations
	frame_quads: FrameQuads,
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
				label: Some("sarthi-renderer-device"),
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

		// Premultiplied, like everything drawn, so the window can be see-through; otherwise whatever the surface offers
		let alpha_mode = if capabilities.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
			wgpu::CompositeAlphaMode::PreMultiplied
		} else {
			capabilities.alpha_modes.first().copied().ok_or(RendererError::NoAlphaMode)?
		};

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

		let quads = QuadPipeline::new(&device, config.format);
		let text = TextPipeline::new(&device, &queue, config.format);
		let shape = ShapePipeline::new(&device, config.format);

		Ok(Self {
			surface,
			device,
			queue,
			config,
			background: renderer_config.background,
			opacity: renderer_config.opacity,
			quads,
			text,
			shape,
			layers: Vec::new(),
			scrolls: ScrollAnimations::new(renderer_config.scroll),
			cursor: CursorAnimation::new(renderer_config.cursor),
			frame_quads: FrameQuads::default(),
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
		self.background = rgb;
	}

	/// Sets how opaque the window's background is, from 0 (see-through) to 1, where the surface allows it. Cells that
	/// should show it draw no background of their own (a `blend` of 100).
	pub fn set_opacity(&mut self, opacity: f32) {
		self.opacity = opacity.clamp(0.0, 1.0);
	}

	/// The color the window is cleared to: the background at its opacity, premultiplied like everything drawn
	fn clear_color(&self) -> wgpu::Color {
		let color = wgpu_color(self.background, self.config.format.is_srgb());
		let opacity = f64::from(self.opacity);
		wgpu::Color { r: color.r * opacity, g: color.g * opacity, b: color.b * opacity, a: opacity }
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

	/// Draws the layers on the clear color, each layer's text in its fonts. Laying out text needs `fonts`, which all
	/// renderers share.
	///
	/// `pre_present` runs right before the frame is handed to the window system, and only if it is, e.g. for
	/// winit's `Window::pre_present_notify`.
	pub fn render(&mut self, fonts: &mut Fonts, pre_present: impl FnOnce()) -> Result<Frame, RendererError> {
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

		let srgb = self.config.format.is_srgb();
		let (surface, now) = ((self.config.width, self.config.height), Instant::now());
		// On its way, the cursor is drawn as its shape, with a block's character only where the shape covers its cell.
		let moving = self.move_cursor(now);
		let mut pieces = pieces(&self.layers, &self.scrolls, surface, now);
		if let Some((corners, cursor)) = moving {
			if let Some(piece) = pieces.last_mut().filter(|piece| piece.kind == PieceKind::Cursor) {
				piece.clip = covered(piece, corners);
			}
			self.shape
				.prepare(&self.queue, surface.0, surface.1, corners, rgba(cursor.color, srgb));
		}

		let resolution = glyphon::Resolution { width: self.config.width, height: self.config.height };
		self.text.prepare(&self.device, &self.queue, resolution, fonts, &pieces)?;

		self.frame_quads.clear();
		self.layer_quads.clear();
		for piece in &pieces {
			let moving = moving.is_some() && piece.kind == PieceKind::Cursor;
			let lines = fonts.line_metrics(piece.layer.font);
			self.layer_quads
				.push(layer_quads(&mut self.frame_quads, piece, &self.text, lines, srgb, moving));
		}
		self.quads.prepare(
			&self.device,
			&self.queue,
			self.config.width,
			self.config.height,
			&self.frame_quads.quads,
			&self.frame_quads.holes,
		);

		let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

		let mut encoder = self
			.device
			.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("sarthi-renderer-encoder") });

		{
			let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
				label: Some("sarthi-renderer-pass"),
				color_attachments: &[Some(wgpu::RenderPassColorAttachment {
					view: &view,
					resolve_target: None,
					depth_slice: None,
					ops: wgpu::Operations {
						load: wgpu::LoadOp::Clear(self.clear_color()),
						store: wgpu::StoreOp::Store,
					},
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
	/// Moves the cursor animation on to the layers' cursor, and gives the cursor and its corners at `now` (in pixels:
	/// top-left, top-right, bottom-right, bottom-left) while it's on its way.
	fn move_cursor(&mut self, now: Instant) -> Option<([[f32; 2]; 4], Cursor)> {
		let (layer, cursor) = self
			.layers
			.iter()
			.find_map(|Layer::Cells(layer)| Some((layer, layer.cursor?)))?;
		self.cursor.update(cursor_rect(layer, cursor), cursor.animate, layer.cell, now);
		if !self.cursor.is_animating(now) {
			return None;
		}
		Some((self.cursor.corners(now)?, cursor))
	}
}

/// The part of the cursor `piece`, within its clip, that the cursor's shape covers while on its way, with `corners` in
/// pixels
fn covered(piece: &Piece, corners: [[f32; 2]; 4]) -> [u32; 4] {
	let [left, top, width, height] = piece.clip;
	let (mut low, mut high) = ([left, top], [left + width, top + height]);
	let [x, y, layer_width, layer_height] = piece.layer.rect();
	let layer = [[x, y], [x + layer_width, y + layer_height]];
	for axis in 0..2 {
		let (least, most) = corners
			.iter()
			.fold((f32::MAX, f32::MIN), |(least, most), corner| (least.min(corner[axis]), most.max(corner[axis])));
		low[axis] = low[axis].max(layer[0][axis]).max(least.floor().max(0.0) as u32);
		high[axis] = high[axis].min(layer[1][axis]).min(most.ceil().max(0.0) as u32);
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

	/// Its layer's rectangle (left, top, width, height in pixels) and corner radius, if the corners are rounded: its
	/// quads are cut to them, whichever part of the layer it draws
	pub round: Option<([f32; 4], f32)>,
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
fn pieces<'a>(layers: &'a [Layer], scrolls: &ScrollAnimations, surface: (u32, u32), now: Instant) -> Vec<Piece<'a>> {
	// A rectangle in pixels (left, top, width, height), cut to the surface: a scissor rectangle can't reach outside it
	let clip = |[x, y, width, height]: [u32; 4]| {
		let (left, top) = (x.min(surface.0), y.min(surface.1));
		let (right, bottom) = ((x + width).min(surface.0), (y + height).min(surface.1));
		[left, top, right - left, bottom - top]
	};

	let mut pieces = Vec::with_capacity(layers.len() + 1);
	let mut cursor_piece = None;
	for Layer::Cells(layer) in layers {
		let whole = clip(layer.rect());
		let round = (layer.corner_radius > 0.0).then(|| (layer.rect().map(|value| value as f32), layer.corner_radius));
		let region = scrolls.region(layer, now);
		// The sliding text is drawn over the layer: where what's under shows through, the layer's own text would too.
		let see_through = layer.cells.iter().any(|cell| cell.blend > 0);
		let base = match &region {
			Some(region) if see_through => Cow::Owned(without(layer, region)),
			_ => Cow::Borrowed(layer),
		};
		pieces.push(Piece { layer: base, kind: PieceKind::Layer, offset: 0.0, clip: whole, round });

		if let Some(region) = region {
			let [x, y, width, _] = region.layer.rect();
			let clip = clip([x, y, width, region.height * layer.cell.height]);
			// Whole pixels keep the text sharp.
			let offset = (region.offset * layer.cell.height as f32).round();
			pieces.push(Piece { layer: Cow::Owned(region.layer), kind: PieceKind::Region, offset, clip, round });
		}

		if let Some(cursor) = layer.cursor {
			let layer = Cow::Owned(cursor_layer(layer, cursor));
			cursor_piece = Some(Piece { layer, kind: PieceKind::Cursor, offset: 0.0, clip: whole, round });
		}
	}
	// Over everything, like in the TUI
	pieces.extend(cursor_piece);
	pieces
}

/// `layer` without the text and backgrounds of its `region` that slides, which is drawn over it
fn without(layer: &CellLayer, region: &SlidingRegion) -> CellLayer {
	let (top, left) = (region.top, region.left);
	let Some(&first) = layer.cells.first() else { return layer.clone() };
	let text = ArrayString::from(" ").unwrap_or_default();
	let blank = RenderCell { text, blend: 100, decoration: Decoration::default(), ..first };
	let mut cells = layer.cells.clone();
	for row in top..(top + region.height).min(layer.height) {
		let start = (row * layer.width + left) as usize;
		cells[start..start + region.layer.width as usize].fill(blank);
	}
	CellLayer { cells, cursor: None, scroll: None, shadow: layer.shadow.clone(), ..*layer }
}

/// The cursor of `layer` as a layer of its own. A block is the cells under it, in the cursor's colors, so its
/// character stays readable; a bar or an outline is drawn over nothing.
fn cursor_layer(layer: &CellLayer, cursor: Cursor) -> CellLayer {
	let (x, y) = (layer.x + cursor.col * layer.cell.width, layer.y + cursor.row * layer.cell.height);
	let empty =
		CellLayer { x, y, width: 0, height: 0, cells: Vec::new(), cursor: None, scroll: None, shadow: None, ..*layer };
	if cursor.shape != CursorShape::Block {
		return CellLayer { x: layer.x, y: layer.y, cursor: Some(cursor), ..empty };
	}

	let start = (cursor.row * layer.width + cursor.col) as usize;
	let Some(cells) = layer.cells.get(start..start + cursor.width as usize) else { return empty };
	let cells = cells
		.iter()
		.map(|cell| RenderCell {
			style: TextStyle { foreground: cursor.text_color, ..cell.style },
			background: cursor.color,
			// The cursor stays opaque, also over a transparent window.
			blend: 0,
			..*cell
		})
		.collect();
	CellLayer { width: cursor.width, height: 1, cells, ..empty }
}

/// A layer's ranges of the frame's quads: its shadow and outline, drawn first and uncut since they reach past the layer;
/// its backgrounds, drawn under its text, and its decorations and bar cursor, drawn over it; and the rectangle these and
/// the text are cut to
struct LayerQuads {
	around: Range<u32>,
	under_text: Range<u32>,
	over_text: Range<u32>,
	clip: [u32; 4],

	/// Whether the cursor's shape is drawn first, uncut: for the cursor on its way
	shape: bool,
}

/// Adds the quads of `piece` to the `frame`'s, with underlines breaking where `text` laid out its text over them. With
/// `moving`, `piece` is the cursor on its way, drawn over its shape.
fn layer_quads(
	frame: &mut FrameQuads,
	piece: &Piece,
	text: &TextPipeline,
	lines: LineMetrics,
	srgb: bool,
	moving: bool,
) -> LayerQuads {
	let layer = &piece.layer;
	let around = frame.quads.len();
	if piece.kind == PieceKind::Layer {
		if let Some(layer_shadow) = &layer.shadow {
			area_shadow(frame, layer_shadow, srgb);
		}
		if let Some(outline) = layer.outline {
			frame.quads.push(outline_quad(layer.rect(), outline, layer.corner_radius, srgb));
		}
	}
	let quads = &mut frame.quads;
	let start = quads.len();
	// The shape of a cursor on its way stands in for its background or bar.
	if !moving {
		cell_backgrounds(quads, layer, srgb);
	}
	let middle = quads.len();
	cell_decorations(quads, layer, lines, srgb, |row| text.ink(piece, row));
	if piece.kind == PieceKind::Cursor && !moving {
		cursor_quad(quads, layer, lines.thickness, srgb);
	}
	for quad in &mut quads[start..] {
		quad.rect[1] += piece.offset;
		// Cut to the layer's rounded corners, which don't move with sliding text
		if let Some((clip, radius)) = piece.round {
			(quad.clip, quad.clip_radius) = (clip, [radius, 0.0]);
		}
	}
	LayerQuads {
		around: around as u32..start as u32,
		under_text: start as u32..middle as u32,
		over_text: middle as u32..quads.len() as u32,
		clip: piece.clip,
		shape: moving,
	}
}

/// Draws the layers one after another, each with its backgrounds, text, then decorations and cursor, so a later
/// layer (e.g. a floating window) covers everything of the earlier ones. A layer's shadow and outline, and the cursor's
/// shape drawn first for the cursor on its way, aren't cut to the layer: on a `surface` of width × height pixels, they reach past it.
fn draw_layers(
	pass: &mut wgpu::RenderPass<'_>,
	quads: &QuadPipeline,
	text: &TextPipeline,
	shape: &ShapePipeline,
	surface: (u32, u32),
	layers: &[LayerQuads],
) -> Result<(), RendererError> {
	for (index, layer) in layers.iter().enumerate() {
		if !layer.around.is_empty() {
			pass.set_scissor_rect(0, 0, surface.0, surface.1);
			quads.render(pass, layer.around.clone());
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
	use crate::quad::{FrameQuads, QuadPipeline};
	use crate::scroll::ScrollAnimations;
	use crate::shape::ShapePipeline;
	use crate::text::TextPipeline;
	use crate::{CellLayer, CellSize, Decoration, FontId, Fonts, Layer, RenderCell, RendererConfig, TextStyle};

	/// Cells of the bundled font at 14 pixels, which the tests draw with
	const CELL: CellSize = CellSize { width: 8, height: 19 };

	/// A one-cell layer at the top-left corner showing `text` in white on `background`
	fn cell(id: u64, text: &str, background: u32) -> Layer {
		let style = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
		let cell = RenderCell {
			text: ArrayString::from(text).unwrap(),
			style,
			background,
			blend: 0,
			decoration: Decoration::default(),
		};
		Layer::Cells(CellLayer {
			id,
			x: 0,
			y: 0,
			cell: CELL,
			font: crate::FontId::MAIN,
			width: 1,
			height: 1,
			cells: vec![cell],
			cursor: None,
			scroll: None,
			shadow: None,
			outline: None,
			corner_radius: 0.0,
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
			let (cell, lines) = (fonts.cell_size(FontId::MAIN), fonts.line_metrics(FontId::MAIN));
			assert_eq!(cell, CELL, "the layers' cells are the font's");
			let mut text = TextPipeline::new(device, queue, format);
			let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
			let pieces = pieces(layers, &scrolls, (width, height), Instant::now());
			let resolution = glyphon::Resolution { width, height };
			text.prepare(device, queue, resolution, &mut fonts, &pieces).unwrap();

			let mut frame = FrameQuads::default();
			let ranges: Vec<_> = pieces
				.iter()
				.map(|piece| layer_quads(&mut frame, piece, &text, lines, false, false))
				.chain(shape.map(|_| super::LayerQuads {
					around: 0..0,
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
			pipeline.prepare(device, queue, width, height, &frame.quads, &frame.holes);

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
		let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
		let pieces = pieces(std::slice::from_ref(&layer), &scrolls, (64, 19), Instant::now());

		let kinds: Vec<_> = pieces.iter().map(|piece| piece.kind).collect();
		assert_eq!(kinds, [PieceKind::Layer, PieceKind::Cursor]);
		// A block: the cell under it, in the cursor's colors, where Neovim put it
		let cursor = &pieces[1].layer;
		assert_eq!((cursor.x, cursor.y, cursor.width, cursor.height, pieces[1].offset), (0, 0, 1, 1, 0.0));
		assert_eq!((cursor.cells[0].text.as_str(), cursor.cells[0].background), ("a", 0xFF0000));
		assert_eq!(cursor.cells[0].style.foreground, 0xFFFFFF);

		// A bar: drawn as a quad, over no cells
		let bar = crate::CursorShape::Bar { orientation: crate::BarOrientation::Vertical, percentage: 25 };
		let layer = with_cursor(cell(1, "a", 0x000000), bar);
		let drawn = super::pieces(std::slice::from_ref(&layer), &scrolls, (64, 19), Instant::now());
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
	fn gpu_a_blended_cell_shows_whats_under_it() {
		let blended = |blend: u8| {
			let Layer::Cells(mut layer) = cell(2, " ", 0xFFFFFF);
			layer.cells[0].blend = blend;
			Layer::Cells(layer)
		};
		// White, half see-through, over red
		let Some(pixel) = middle_pixel(&[cell(1, " ", 0xFF0000), blended(50)]) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		assert!(pixel[0] == 255 && (126..=129).contains(&pixel[1]) && pixel[1] == pixel[2], "{pixel:?}");
		// All the way see-through: only the red
		assert_eq!(middle_pixel(&[cell(1, " ", 0xFF0000), blended(100)]).unwrap(), [255, 0, 0, 255]);
	}

	#[test]
	fn gpu_a_shadow_isnt_drawn_under_what_casts_it() {
		// A see-through cell at column 3, drawing no background, with a red shadow, on black
		let shadow = crate::Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0xFF0000, opacity: 1.0 };
		let Layer::Cells(mut clear) = cell(1, " ", 0xFFFFFF);
		clear.cells[0].blend = 100;
		let area = vec![[3 * CELL.width, 0, CELL.width, CELL.height]];
		let shadow = crate::LayerShadow { shadow, area, radius: 0.0 };
		let layer = Layer::Cells(CellLayer { x: 3 * CELL.width, shadow: Some(shadow), ..clear });
		let Some((pixels, cell)) = drawn(&[layer], None) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		let at = |x: u32| pixels[(9 * WIDTH + x) as usize];
		// Under the cell, nothing: like CSS's box-shadow, only outside it
		assert_eq!(at(3 * cell.width + cell.width / 2), [0, 0, 0, 255]);
		// Around it, the shadow
		assert!(at(3 * cell.width - 1)[0] > 50, "{:?}", at(3 * cell.width - 1));
	}

	#[test]
	fn gpu_a_shadow_fades_out_around_its_layer() {
		// A white cell at column 3, with a red shadow, on black
		let shadow = crate::Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0xFF0000, opacity: 1.0 };
		let Layer::Cells(white) = cell(1, " ", 0xFFFFFF);
		let area = vec![[3 * CELL.width, 0, CELL.width, CELL.height]];
		let shadow = crate::LayerShadow { shadow, area, radius: 0.0 };
		let layer = Layer::Cells(CellLayer { x: 3 * CELL.width, shadow: Some(shadow), ..white });
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
	fn gpu_an_outline_goes_around_its_layer() {
		// A white cell at column 3, with a 1 px red outline, on black
		let Layer::Cells(white) = cell(1, " ", 0xFFFFFF);
		let outline = crate::Outline { width: 1.0, color: 0xFF0000 };
		let layer = Layer::Cells(CellLayer { x: 3 * CELL.width, outline: Some(outline), ..white });
		let Some((pixels, cell)) = drawn(&[layer], None) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		let at = |x: u32| pixels[(9 * WIDTH + x) as usize];
		let left = 3 * cell.width;
		// Just outside the cell, the line; further out, nothing; the cell itself keeps its own pixels.
		assert_eq!((at(left - 1), at(left - 2), at(left)), ([255, 0, 0, 255], [0, 0, 0, 255], [255, 255, 255, 255]));
		assert_eq!(at(left + cell.width), [255, 0, 0, 255]);
	}

	#[test]
	fn gpu_rounded_corners_cut_the_layer() {
		// A white cell at column 3 with corners rounded by 3 px, on black
		let Layer::Cells(white) = cell(1, " ", 0xFFFFFF);
		let layer = Layer::Cells(CellLayer { x: 3 * CELL.width, corner_radius: 3.0, ..white });
		let Some((pixels, cell)) = drawn(&[layer], None) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		let at = |x: u32, y: u32| pixels[(y * WIDTH + x) as usize];
		let left = 3 * cell.width;
		// The corner pixel is outside the curve; along the edges and in the middle, the cell.
		assert_eq!(at(left, 0), [0, 0, 0, 255]);
		assert_eq!(at(left + cell.width - 1, 18), [0, 0, 0, 255]);
		assert_eq!((at(left + 4, 0), at(left, 9)), ([255, 255, 255, 255], [255, 255, 255, 255]));
	}

	#[test]
	fn the_cursor_is_drawn_over_every_layer() {
		let lower = with_cursor(cell(1, "a", 0x000000), crate::CursorShape::Block);
		let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
		let layers = [lower, cell(2, "b", 0x000000)];
		let pieces = pieces(&layers, &scrolls, (64, 19), Instant::now());
		let kinds: Vec<_> = pieces.iter().map(|piece| (piece.layer.id, piece.kind)).collect();
		assert_eq!(kinds, [(1, PieceKind::Layer), (2, PieceKind::Layer), (1, PieceKind::Cursor)]);
	}

	#[test]
	fn a_moving_block_shows_its_character_where_its_shape_covers_it() {
		let layer = with_cursor(cell(1, "a", 0x000000), crate::CursorShape::Block);
		let scrolls = ScrollAnimations::new(RendererConfig::default().scroll);
		let pieces = pieces(std::slice::from_ref(&layer), &scrolls, (64, 19), Instant::now());
		let cursor = &pieces[1];

		// The shape reaches halfway into the cell from the right: the right half shows.
		let arriving = [[4.0, 0.0], [30.0, 0.0], [30.0, 19.0], [4.0, 19.0]];
		assert_eq!(super::covered(cursor, arriving), [4, 0, 4, 19]);
		// Not there yet: nothing shows.
		let away = [[20.0, 0.0], [30.0, 0.0], [30.0, 19.0], [20.0, 19.0]];
		assert_eq!(super::covered(cursor, away)[2], 0);
	}

	#[test]
	fn gpu_an_outline_cursor_leaves_its_cell_as_it_is() {
		// A red outline around a black cell
		let layer = with_cursor(cell(1, " ", 0x000000), crate::CursorShape::Outline);
		let Some((pixels, cell)) = drawn(&[layer], None) else {
			eprintln!("skipped: no GPU adapter");
			return;
		};
		let at = |x: u32, y: u32| pixels[(y * WIDTH + x) as usize];
		assert_eq!(at(cell.width / 2, 9), [0, 0, 0, 255]);
		assert_eq!((at(0, 9), at(cell.width / 2, 0)), ([255, 0, 0, 255], [255, 0, 0, 255]));
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
			let scroll =
				scroll.then_some(crate::Scroll { rows: 1, top: 0, bottom: 3, left: 0, right: 1, motion: None });
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

		let later = now + std::time::Duration::from_millis(50);
		let drawn = pieces(std::slice::from_ref(&scrolled), &scrolls, (8, 57), later);
		let kinds: Vec<_> = drawn.iter().map(|piece| piece.kind).collect();
		assert_eq!(kinds, [PieceKind::Layer, PieceKind::Region, PieceKind::Cursor]);
		// The text is still sliding, but the cursor is on the last row, on "d", where Neovim put it.
		assert!(drawn[1].offset != 0.0, "{}", drawn[1].offset);
		assert_eq!(drawn[2].offset, 0.0);
		assert_eq!((drawn[2].layer.y, drawn[2].layer.cells[0].text.as_str()), (2 * CELL.height, "d"));
	}

	#[test]
	fn under_see_through_sliding_text_the_layer_leaves_the_region_out() {
		let column = |rows: [&str; 3], scroll: bool, blend: u8| {
			let Layer::Cells(top) = cell(1, rows[0], 0x000000);
			let cells =
				rows.iter()
					.map(|text| RenderCell { text: ArrayString::from(text).unwrap(), blend, ..top.cells[0] });
			let scroll =
				scroll.then_some(crate::Scroll { rows: 1, top: 1, bottom: 3, left: 0, right: 1, motion: None });
			Layer::Cells(CellLayer { height: 3, cells: cells.collect(), scroll, ..top })
		};
		let layer_text = |blend: u8| {
			let (shown, scrolled) = (column(["x", "b", "c"], false, blend), column(["x", "c", "d"], true, blend));
			let (mut scrolls, now) = (ScrollAnimations::new(RendererConfig::default().scroll), Instant::now());
			scrolls.update(std::slice::from_ref(&shown), std::slice::from_ref(&scrolled), now);
			let later = now + std::time::Duration::from_millis(50);
			let drawn = pieces(std::slice::from_ref(&scrolled), &scrolls, (8, 57), later);
			assert_eq!(drawn[1].kind, PieceKind::Region);
			drawn[0]
				.layer
				.cells
				.iter()
				.map(|cell| (cell.text.as_str().to_owned(), cell.blend))
				.collect::<Vec<_>>()
		};

		// See-through: the region's rows (1 and 2) are blank and draw no background; the row above stays.
		let blank = (String::from(" "), 100);
		assert_eq!(layer_text(100), [(String::from("x"), 100), blank.clone(), blank]);
		// Opaque: the layer is drawn whole, and the region covers it.
		let whole: Vec<_> = ["x", "c", "d"].map(|text| (String::from(text), 0)).to_vec();
		assert_eq!(layer_text(0), whole);
	}
}
