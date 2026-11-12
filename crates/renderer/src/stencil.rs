//! The stencil buffer that cuts a layer's text to its rounded corners: before the text, a quad over the layer writes the
//! layer's reference where its rounded rectangle is ([`QuadPipeline::render_mask`](crate::quad::QuadPipeline)), and the
//! text is drawn only where the stencil holds it. Everything else ignores the stencil, but every pipeline in a pass with
//! the stencil attached has to say so.

pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Stencil8;

/// Ignores the stencil
pub(crate) fn ignored() -> wgpu::DepthStencilState {
	state(wgpu::CompareFunction::Always, wgpu::StencilOperation::Keep, 0)
}

/// Writes the reference wherever it draws
pub(crate) fn writes() -> wgpu::DepthStencilState {
	state(wgpu::CompareFunction::Always, wgpu::StencilOperation::Replace, 0xFF)
}

/// Draws only where the stencil holds the reference
pub(crate) fn tested() -> wgpu::DepthStencilState {
	state(wgpu::CompareFunction::Equal, wgpu::StencilOperation::Keep, 0)
}

fn state(compare: wgpu::CompareFunction, pass_op: wgpu::StencilOperation, write_mask: u32) -> wgpu::DepthStencilState {
	let face = wgpu::StencilFaceState {
		compare,
		fail_op: wgpu::StencilOperation::Keep,
		depth_fail_op: wgpu::StencilOperation::Keep,
		pass_op,
	};
	wgpu::DepthStencilState {
		format: FORMAT,
		depth_write_enabled: Some(false),
		depth_compare: Some(wgpu::CompareFunction::Always),
		stencil: wgpu::StencilState { front: face, back: face, read_mask: 0xFF, write_mask },
		bias: wgpu::DepthBiasState::default(),
	}
}

/// A stencil buffer for a surface of `width` × `height` pixels
pub(crate) fn view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
	device
		.create_texture(&wgpu::TextureDescriptor {
			label: Some("sarthi-stencil"),
			size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
			mip_level_count: 1,
			sample_count: 1,
			dimension: wgpu::TextureDimension::D2,
			format: FORMAT,
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
			view_formats: &[],
		})
		.create_view(&wgpu::TextureViewDescriptor::default())
}

/// `view` attached to a pass, cleared to 0, which no layer's reference is
pub(crate) fn attachment(view: &wgpu::TextureView) -> wgpu::RenderPassDepthStencilAttachment<'_> {
	wgpu::RenderPassDepthStencilAttachment {
		view,
		depth_ops: None,
		stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
	}
}

/// The reference of the layer at `index`: from 1, wrapping past 255
pub(crate) fn reference(index: usize) -> u32 {
	(index % 255) as u32 + 1
}
