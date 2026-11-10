// Rectangles with optional rounded corners and border, patterned lines (curly, dotted and dashed underlines), or soft
// shadows, one instance per quad. Positions are in physical pixels from the top-left corner of the surface.

// Surface size in pixels (xy); zw is padding
@group(0) @binding(0) var<uniform> viewport: vec4<f32>;

struct Quad {
	// Left, top, width, height
	@location(0) rect: vec4<f32>,
	@location(1) color: vec4<f32>,
	@location(2) border_color: vec4<f32>,
	// Corner radius, border width
	@location(3) radius_border: vec2<f32>,
	// Kind (0: none, 1: wave, 2: dots, 3: dashes), period and line thickness in pixels, unused; or for a shadow
	// (kind 4), its blur radius in pixels
	@location(4) pattern: vec4<f32>,
}

struct VertexOutput {
	@builtin(position) position: vec4<f32>,
	// Pixel position relative to the quad's center
	@location(0) local: vec2<f32>,
	@location(1) @interpolate(flat) half_size: vec2<f32>,
	@location(2) @interpolate(flat) color: vec4<f32>,
	@location(3) @interpolate(flat) border_color: vec4<f32>,
	@location(4) @interpolate(flat) radius_border: vec2<f32>,
	@location(5) @interpolate(flat) pattern: vec4<f32>,
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
	out.pattern = quad.pattern;
	return out;
}

// Signed distance from `p` to a box of `half_size` centered on the origin, with corners rounded by `radius`:
// negative inside, positive outside
fn rounded_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
	let q = abs(p) - half_size + radius;
	return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

// How much of this pixel a patterned line covers. The pattern follows the surface x, so it continues across
// neighbouring quads.
fn pattern_coverage(in: VertexOutput) -> f32 {
	let period = in.pattern.y;
	let thickness = in.pattern.z;
	let x = in.position.x;

	switch u32(in.pattern.x + 0.5) {
		// A sine wave as high as the quad
		case 1u: {
			let amplitude = max(in.half_size.y - thickness * 0.5, 0.0);
			let phase = 6.2831853 * x / period;
			let slope = amplitude * 6.2831853 / period * cos(phase);
			// Roughly the distance to the curve: the vertical distance, shortened where the curve is steep
			let distance = abs(in.local.y - amplitude * sin(phase)) / sqrt(1.0 + slope * slope);
			return clamp(thickness * 0.5 + 0.5 - distance, 0.0, 1.0);
		}
		// Dots and dashes: on for the first half of each period
		case 2u, 3u: {
			return select(0.0, 1.0, fract(x / period) < 0.5);
		}
		default: {
			return 1.0;
		}
	}
}

// The error function, to within 5e-4 (Abramowitz and Stegun 7.1.27), for both values of `x`
fn erf(x: vec2<f32>) -> vec2<f32> {
	let s = sign(x);
	let a = abs(x);
	var y = 1.0 + (0.278393 + (0.230389 + 0.078108 * a * a) * a) * a;
	y = y * y;
	return s - s / (y * y);
}

// How much of a box shadow covers this pixel: a box of `half_size` blurred with a Gaussian of standard deviation
// `sigma`, which is exact for a box (the blur is separable). The quad reaches 3 sigma past the box on each side.
fn shadow_coverage(in: VertexOutput, sigma: f32) -> f32 {
	let half = max(in.half_size - 3.0 * sigma, vec2<f32>(0.0));
	let scale = 1.0 / (sqrt(2.0) * sigma);
	let both = 0.5 * (erf((in.local + half) * scale) - erf((in.local - half) * scale));
	return both.x * both.y;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	if u32(in.pattern.x + 0.5) == 4u {
		let sigma = max(in.pattern.y * 0.5, 0.01);
		return vec4<f32>(in.color.rgb * in.color.a, in.color.a) * shadow_coverage(in, sigma);
	}
	if in.pattern.x > 0.5 {
		return vec4<f32>(in.color.rgb * in.color.a, in.color.a) * pattern_coverage(in);
	}

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
