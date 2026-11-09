use bytemuck::{Pod, Zeroable};

use crate::color::rgba;
use crate::{CellSize, Layer, RenderCell};

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
}

impl Quad {
	/// A plain rectangle without rounded corners or border
	pub fn fill(rect: [f32; 4], color: [f32; 4]) -> Self {
		Self { rect, color, border_color: [0.0; 4], corner_radius: 0.0, border_width: 0.0 }
	}
}

/// Draws [`Quad`]s in one instanced draw call
pub(crate) struct QuadPipeline {
	pipeline: wgpu::RenderPipeline,
	viewport: wgpu::Buffer,
	bind_group: wgpu::BindGroup,

	/// Instance buffer, grown when a frame needs more room, and the number of quads it holds
	instances: wgpu::Buffer,
	count: u32,
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
					attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x2],
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
		Self { pipeline, viewport, bind_group, instances, count: 0 }
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
		self.count = quads.len() as u32;
	}

	pub(crate) fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
		if self.count == 0 {
			return;
		}
		pass.set_pipeline(&self.pipeline);
		pass.set_bind_group(0, &self.bind_group, &[]);
		pass.set_vertex_buffer(0, self.instances.slice(..));
		pass.draw(0..4, 0..self.count);
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

/// Adds a quad for each run of cells with the same background in `layers`, in layer order. Default backgrounds are
/// drawn too, so a layer covers whatever is below it.
pub(crate) fn cell_backgrounds(quads: &mut Vec<Quad>, layers: &[Layer], cell: CellSize, srgb_surface: bool) {
	let (width, height) = (cell.width as f32, cell.height as f32);

	for Layer::Cells(layer) in layers {
		for (row, cells) in (layer.row..).zip(layer.cells.chunks_exact((layer.width as usize).max(1))) {
			for (col, len, color) in background_runs(cells) {
				let rect = [(layer.col + col) as f32 * width, row as f32 * height, len as f32 * width, height];
				quads.push(Quad::fill(rect, rgba(color, srgb_surface)));
			}
		}
	}
}

/// Runs of neighbouring cells with the same background, as (first column, length, color)
fn background_runs(cells: &[RenderCell]) -> impl Iterator<Item = (u32, u32, u32)> {
	cells.chunk_by(|a, b| a.background == b.background).scan(0, |col, run| {
		let start = *col;
		*col += run.len() as u32;
		Some((start, run.len() as u32, run[0].background))
	})
}

#[cfg(test)]
mod tests {
	use arrayvec::ArrayString;

	use super::{Quad, QuadPipeline, background_runs};
	use crate::{RenderCell, TextStyle};

	fn cells(backgrounds: &[u32]) -> Vec<RenderCell> {
		let style = TextStyle { foreground: 0xFFFFFF, bold: false, italic: false };
		backgrounds
			.iter()
			.map(|&background| RenderCell { text: ArrayString::from("x").unwrap(), style, background })
			.collect()
	}

	#[test]
	fn runs_merge_equal_backgrounds() {
		let row = cells(&[1, 1, 2, 2, 2, 1]);
		assert_eq!(background_runs(&row).collect::<Vec<_>>(), [(0, 2, 1), (2, 3, 2), (5, 1, 1)]);
	}

	#[test]
	fn runs_of_a_plain_row() {
		assert_eq!(background_runs(&cells(&[7; 4])).collect::<Vec<_>>(), [(0, 4, 7)]);
		assert_eq!(background_runs(&[]).count(), 0);
	}

	#[test]
	fn double_width_halves_keep_their_backgrounds() {
		let mut row = cells(&[1, 2, 2, 1]);
		row[2].text = ArrayString::new();
		assert_eq!(background_runs(&row).collect::<Vec<_>>(), [(0, 1, 1), (1, 2, 2), (3, 1, 1)]);
	}

	const SIZE: u32 = 64;

	/// Draws `quads` on black into a `SIZE`×`SIZE` texture and returns its pixels row by row, or `None` without a GPU
	fn draw(quads: &[Quad]) -> Option<Vec<[u8; 4]>> {
		smol::block_on(async {
			let instance = wgpu::Instance::default();
			let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.ok()?;
			let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.ok()?;

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

			let mut pipeline = QuadPipeline::new(&device, format);
			pipeline.prepare(&device, &queue, SIZE, SIZE, quads);

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
				pipeline.render(&mut pass);
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
}
