// A filled shape with four corners, like the cursor while it stretches on its way: any four corners in order around
// it, so not only rectangles. Positions are in physical pixels from the top-left corner of the surface.

struct Shape {
	// Surface size in pixels (xy); zw is padding
	viewport: vec4<f32>,
	// The corners in order: (x, y) of the first and second, then of the third and fourth
	corners: array<vec4<f32>, 2>,
	// Not premultiplied
	color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> shape: Shape;

struct VertexOutput {
	@builtin(position) position: vec4<f32>,
	@location(0) pixel: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32) -> VertexOutput {
	let a = shape.corners[0].xy;
	let b = shape.corners[0].zw;
	let c = shape.corners[1].xy;
	let d = shape.corners[1].zw;
	// The box around the corners, a pixel bigger for the anti-aliased edge, drawn as a triangle strip
	let low = min(min(a, b), min(c, d)) - 1.0;
	let high = max(max(a, b), max(c, d)) + 1.0;
	let pixel = mix(low, high, vec2<f32>(f32(vertex & 1u), f32(vertex >> 1u)));

	var out: VertexOutput;
	out.position = vec4<f32>(pixel / shape.viewport.xy * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
	out.pixel = pixel;
	return out;
}

// Signed distance from `p` to the triangle `p0`, `p1`, `p2`: negative inside, positive outside
fn triangle(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
	let e0 = p1 - p0;
	let e1 = p2 - p1;
	let e2 = p0 - p2;
	let v0 = p - p0;
	let v1 = p - p1;
	let v2 = p - p2;
	// The nearest point on each edge; an edge of no length is its point.
	let q0 = v0 - e0 * clamp(dot(v0, e0) / max(dot(e0, e0), 1e-6), 0.0, 1.0);
	let q1 = v1 - e1 * clamp(dot(v1, e1) / max(dot(e1, e1), 1e-6), 0.0, 1.0);
	let q2 = v2 - e2 * clamp(dot(v2, e2) / max(dot(e2, e2), 1e-6), 0.0, 1.0);
	// Which side of each edge `p` is on, the same way round whichever way the triangle winds
	let winding = sign(e0.x * e2.y - e0.y * e2.x);
	let side = min(min(winding * (v0.x * e0.y - v0.y * e0.x), winding * (v1.x * e1.y - v1.y * e1.x)),
		winding * (v2.x * e2.y - v2.y * e2.x));
	let distance = sqrt(min(min(dot(q0, q0), dot(q1, q1)), dot(q2, q2)));
	return select(distance, -distance, side > 0.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let a = shape.corners[0].xy;
	let b = shape.corners[0].zw;
	let c = shape.corners[1].xy;
	let d = shape.corners[1].zw;
	// As two triangles, so a shape that's briefly not convex still fills
	let distance = min(triangle(in.pixel, a, b, c), triangle(in.pixel, a, c, d));
	// 1 inside, 0 outside, a ramp across the edge (anti-aliasing)
	let coverage = clamp(0.5 - distance, 0.0, 1.0);
	// Premultiplied alpha
	return vec4<f32>(shape.color.rgb * shape.color.a, shape.color.a) * coverage;
}
