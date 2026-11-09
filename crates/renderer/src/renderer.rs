use std::ops::Range;

use crate::color::wgpu_color;
use crate::font::LineMetrics;
use crate::quad::{Quad, QuadPipeline, bar_cursor, cell_backgrounds, cell_decorations};
use crate::text::TextPipeline;
use crate::{CellLayer, CellSize, Fonts, Layer, RendererConfig, RendererError};

pub struct Renderer {
	surface: wgpu::Surface<'static>,
	device: wgpu::Device,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	clear_color: wgpu::Color,
	quads: QuadPipeline,
	text: TextPipeline,

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

		let alpha_mode =
			capabilities.alpha_modes.first().copied().ok_or(RendererError::NoAlphaMode)?;

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

		Ok(Self {
			surface,
			device,
			queue,
			config,
			clear_color,
			quads,
			text,
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

	/// Draws `layers` on the clear color, in order, with text of `font_size` physical pixels. Laying out text
	/// needs `fonts`, which all renderers share.
	///
	/// `pre_present` runs right before the frame is handed to the window system, and only if it is, e.g. for
	/// winit's `Window::pre_present_notify`.
	pub fn render(
		&mut self,
		fonts: &mut Fonts,
		layers: &[Layer],
		font_size: f32,
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

		let resolution = glyphon::Resolution { width: self.config.width, height: self.config.height };
		self.text
			.prepare(&self.device, &self.queue, resolution, fonts, layers, font_size)?;

		let (cell, lines, srgb) =
			(fonts.cell_size(font_size), fonts.line_metrics(font_size), self.config.format.is_srgb());
		self.frame_quads.clear();
		self.layer_quads.clear();
		for Layer::Cells(layer) in layers {
			self.layer_quads
				.push(layer_quads(&mut self.frame_quads, layer, cell, lines, srgb));
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
					ops: wgpu::Operations {
						load: wgpu::LoadOp::Clear(self.clear_color),
						store: wgpu::StoreOp::Store,
					},
				})],
				depth_stencil_attachment: None,
				occlusion_query_set: None,
				timestamp_writes: None,
				multiview_mask: None,
			});
			draw_layers(&mut pass, &self.quads, &self.text, &self.layer_quads)?;
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

/// A layer's ranges of the frame's quads: its backgrounds, drawn under its text, and its decorations and bar cursor,
/// drawn over it
struct LayerQuads {
	under_text: Range<u32>,
	over_text: Range<u32>,
}

/// Adds the quads of `layer` to `quads`
fn layer_quads(quads: &mut Vec<Quad>, layer: &CellLayer, cell: CellSize, lines: LineMetrics, srgb: bool) -> LayerQuads {
	let start = quads.len() as u32;
	cell_backgrounds(quads, layer, cell, srgb);
	let middle = quads.len() as u32;
	cell_decorations(quads, layer, cell, lines, srgb);
	bar_cursor(quads, layer, cell, srgb);
	LayerQuads { under_text: start..middle, over_text: middle..quads.len() as u32 }
}

/// Draws the layers one after another, each with its backgrounds, text, then decorations and cursor, so a later
/// layer (e.g. a floating window) covers everything of the earlier ones
fn draw_layers(
	pass: &mut wgpu::RenderPass<'_>,
	quads: &QuadPipeline,
	text: &TextPipeline,
	layers: &[LayerQuads],
) -> Result<(), RendererError> {
	for (index, layer) in layers.iter().enumerate() {
		quads.render(pass, layer.under_text.clone());
		text.render(pass, index)?;
		quads.render(pass, layer.over_text.clone());
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use arrayvec::ArrayString;

	use super::{draw_layers, layer_quads};
	use crate::quad::QuadPipeline;
	use crate::text::TextPipeline;
	use crate::{CellLayer, Decoration, Fonts, Layer, RenderCell, TextStyle};

	/// A one-cell layer at the top-left corner showing `text` in white on `background`
	fn cell(id: u64, text: &str, background: u32) -> Layer {
		let style = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
		let cell =
			RenderCell { text: ArrayString::from(text).unwrap(), style, background, decoration: Decoration::default() };
		Layer::Cells(CellLayer { id, row: 0, col: 0, width: 1, height: 1, cells: vec![cell], cursor: None })
	}

	/// Draws `layers` like the renderer does, into an 8×19 texture, and returns the pixel in the middle of the cell, or
	/// `None` without a GPU
	fn middle_pixel(layers: &[Layer]) -> Option<[u8; 4]> {
		let (width, height) = (64, 19);
		smol::block_on(async {
			let instance = wgpu::Instance::default();
			let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.ok()?;
			let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.ok()?;
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
			let font_size = 14.0;
			let (cell, lines) = (fonts.cell_size(font_size), fonts.line_metrics(font_size));
			let mut text = TextPipeline::new(&device, &queue, format);
			let resolution = glyphon::Resolution { width, height };
			text.prepare(&device, &queue, resolution, &mut fonts, layers, font_size)
				.unwrap();

			let mut quads = Vec::new();
			let ranges: Vec<_> = layers
				.iter()
				.map(|Layer::Cells(layer)| layer_quads(&mut quads, layer, cell, lines, false))
				.collect();
			let mut pipeline = QuadPipeline::new(&device, format);
			pipeline.prepare(&device, &queue, width, height, &quads);

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
				draw_layers(&mut pass, &pipeline, &text, &ranges).unwrap();
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
			Some(pixels[(9 * width + cell.width / 2) as usize])
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
}
