// Rectangles with optional rounded corners and border, one instance per rectangle. Positions are in physical
// pixels from the top-left corner of the surface.

// Surface size in pixels (xy); zw is padding
@group(0) @binding(0) var<uniform> viewport: vec4<f32>;

struct Quad {
	// Left, top, width, height
	@location(0) rect: vec4<f32>,
	@location(1) color: vec4<f32>,
	@location(2) border_color: vec4<f32>,
	// Corner radius, border width
	@location(3) radius_border: vec2<f32>,
}

struct VertexOutput {
	@builtin(position) position: vec4<f32>,
	// Pixel position relative to the quad's center
	@location(0) local: vec2<f32>,
	@location(1) @interpolate(flat) half_size: vec2<f32>,
	@location(2) @interpolate(flat) color: vec4<f32>,
	@location(3) @interpolate(flat) border_color: vec4<f32>,
	@location(4) @interpolate(flat) radius_border: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, quad: Quad) -> VertexOutput {
	// Corners (0, 0), (1, 0), (0, 1), (1, 1), drawn as a triangle strip
	let corner = vec2<f32>(f32(vertex & 1u), f32(vertex >> 1u));
	let pixel = quad.rect.xy + corner * quad.rect.zw;

	var out: VertexOutput;
	out.position = vec4<f32>(pixel / viewport.xy * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
	out.local = (corner - 0.5) * quad.rect.zw;
	out.half_size = quad.rect.zw * 0.5;
	out.color = quad.color;
	out.border_color = quad.border_color;
	out.radius_border = quad.radius_border;
	return out;
}

// Signed distance from `p` to a box of `half_size` centered on the origin, with corners rounded by `radius`:
// negative inside, positive outside
fn rounded_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
	let q = abs(p) - half_size + radius;
	return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let radius = min(in.radius_border.x, min(in.half_size.x, in.half_size.y));
	let border = in.radius_border.y;

	// How much of this pixel the shape covers: 1 inside, 0 outside, a ramp across the edge (anti-aliasing).
	// Pixel centers are half a pixel in, so a square edge on a pixel boundary stays sharp.
	let coverage = clamp(0.5 - rounded_box(in.local, in.half_size, radius), 0.0, 1.0);

	var color = in.color;
	if border > 0.0 {
		let inner = rounded_box(in.local, in.half_size - border, max(radius - border, 0.0));
		color = mix(in.color, in.border_color, clamp(0.5 + inner, 0.0, 1.0));
	}

	// Premultiplied alpha
	return vec4<f32>(color.rgb * color.a, color.a) * coverage;
}
