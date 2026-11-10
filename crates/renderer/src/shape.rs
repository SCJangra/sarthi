use bytemuck::{Pod, Zeroable};

/// What [`ShapePipeline`] draws: see `shaders/shape.wgsl`
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
struct Shape {
	viewport: [f32; 4],
	corners: [[f32; 2]; 4],

	/// RGBA as the surface expects it (see [`crate::color::rgba`]), not premultiplied
	color: [f32; 4],
}

/// Draws one filled shape with four corners, like the cursor while it moves
pub(crate) struct ShapePipeline {
	pipeline: wgpu::RenderPipeline,
	shape: wgpu::Buffer,
	bind_group: wgpu::BindGroup,
}

impl ShapePipeline {
	pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
		let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/shape.wgsl"));

		let shape = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("sarthi-shape"),
			size: size_of::<Shape>() as u64,
			usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			label: Some("sarthi-shape-bind-group-layout"),
			entries: &[wgpu::BindGroupLayoutEntry {
				binding: 0,
				visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
				ty: wgpu::BindingType::Buffer {
					ty: wgpu::BufferBindingType::Uniform,
					has_dynamic_offset: false,
					min_binding_size: None,
				},
				count: None,
			}],
		});

		let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			label: Some("sarthi-shape-bind-group"),
			layout: &bind_group_layout,
			entries: &[wgpu::BindGroupEntry { binding: 0, resource: shape.as_entire_binding() }],
		});

		let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
			label: Some("sarthi-shape-pipeline-layout"),
			bind_group_layouts: &[Some(&bind_group_layout)],
			..Default::default()
		});

		let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("sarthi-shape-pipeline"),
			layout: Some(&layout),
			vertex: wgpu::VertexState {
				module: &shader,
				entry_point: Some("vs_main"),
				buffers: &[],
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

		Self { pipeline, shape, bind_group }
	}

	/// Uploads the shape with `corners` (in pixels, in order around it) in `color`, for a surface of `width` ×
	/// `height` pixels.
	pub(crate) fn prepare(
		&self,
		queue: &wgpu::Queue,
		width: u32,
		height: u32,
		corners: [[f32; 2]; 4],
		color: [f32; 4],
	) {
		let shape = Shape { viewport: [width as f32, height as f32, 0.0, 0.0], corners, color };
		queue.write_buffer(&self.shape, 0, bytemuck::bytes_of(&shape));
	}

	/// Draws the shape last prepared.
	pub(crate) fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
		pass.set_pipeline(&self.pipeline);
		pass.set_bind_group(0, &self.bind_group, &[]);
		pass.draw(0..4, 0..1);
	}
}
