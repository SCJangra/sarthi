use crate::color::wgpu_color;
use crate::quad::{Quad, QuadPipeline, bar_cursors, cell_backgrounds};
use crate::text::TextPipeline;
use crate::{Fonts, Layer, RendererConfig, RendererError};

pub struct Renderer {
	surface: wgpu::Surface<'static>,
	device: wgpu::Device,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	clear_color: wgpu::Color,
	quads: QuadPipeline,
	text: TextPipeline,

	/// Quads of the current frame, kept to reuse the allocation: cell backgrounds, then bar cursors
	frame_quads: Vec<Quad>,
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

		Ok(Self { surface, device, queue, config, clear_color, quads, text, frame_quads: Vec::new() })
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

		let (cell, srgb) = (fonts.cell_size(font_size), self.config.format.is_srgb());
		self.frame_quads.clear();
		cell_backgrounds(&mut self.frame_quads, layers, cell, srgb);
		let backgrounds = 0..self.frame_quads.len() as u32;
		bar_cursors(&mut self.frame_quads, layers, cell, srgb);
		let cursors = backgrounds.end..self.frame_quads.len() as u32;
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
			self.quads.render(&mut pass, backgrounds);
			self.text.render(&mut pass)?;
			self.quads.render(&mut pass, cursors);
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
