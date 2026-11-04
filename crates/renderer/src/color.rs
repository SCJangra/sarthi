/// Converts a color as Neovim sends it, `0xRRGGBB` in sRGB, to a [`wgpu::Color`] for a surface of the
/// given kind.
///
/// wgpu expects linear values when it writes to an sRGB surface (it encodes them back to sRGB), so the
/// sRGB transfer function is undone first. A non-sRGB surface stores values as they are, so those are
/// passed through.
pub(crate) fn wgpu_color(rgb: u32, srgb_surface: bool) -> wgpu::Color {
	let channel = |shift: u32| {
		let value = f64::from((rgb >> shift) & 0xFF) / 255.0;
		if srgb_surface { srgb_to_linear(value) } else { value }
	};

	wgpu::Color { r: channel(16), g: channel(8), b: channel(0), a: 1.0 }
}

/// The inverse sRGB transfer function (IEC 61966-2-1), for a channel value in `0.0..=1.0`
fn srgb_to_linear(value: f64) -> f64 {
	if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

#[cfg(test)]
mod tests {
	use super::wgpu_color;

	fn assert_close(actual: f64, expected: f64) {
		assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
	}

	#[test]
	fn black_and_white() {
		assert_eq!(wgpu_color(0x000000, true), wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 });
		assert_eq!(wgpu_color(0xFFFFFF, true), wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 });
	}

	#[test]
	fn channels_are_in_rgb_order() {
		let color = wgpu_color(0xFF0080, false);
		assert_close(color.r, 1.0);
		assert_close(color.g, 0.0);
		assert_close(color.b, 128.0 / 255.0);
	}

	#[test]
	fn srgb_surface_gets_linear_values() {
		// sRGB 128/255 is 0.2158605 in linear light.
		assert_close(wgpu_color(0x808080, true).r, 0.215_860_5);
		// Below the 0.04045 threshold the curve is linear: (10/255) / 12.92.
		assert_close(wgpu_color(0x0A0A0A, true).r, 10.0 / 255.0 / 12.92);
		// Neovim's default background in the recorded corpus: 0x1C1C1C, 28/255.
		assert_close(wgpu_color(0x1C1C1C, true).r, 0.011_612_2);
	}

	#[test]
	fn other_surfaces_get_values_as_they_are() {
		assert_close(wgpu_color(0x808080, false).r, 128.0 / 255.0);
		assert_close(wgpu_color(0x1C1C1C, false).g, 28.0 / 255.0);
	}
}
