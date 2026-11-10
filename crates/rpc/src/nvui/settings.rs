//! nvui's settings, set from Lua with `require("nvui").setup({ … })`. Each section and value is optional: what isn't
//! given keeps its default. Fonts aren't here: they're Neovim's own `guifont` and `linespace` options.

use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
	pub scroll: ScrollSettings,
	pub underline: UnderlineSettings,
	pub touchpad: TouchpadSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollSettings {
	/// How text that scrolled moves to its new place
	pub animation: Animation,

	/// Windows showing these filetypes jump to their new place, e.g. menus whose selected line is highlighted text. A
	/// window's `w:nvui_smooth_scroll` overrides this.
	pub exclude_filetypes: Vec<String>,
}

/// An animation's curve, e.g. `{ curve = "spring", stiffness = 400, damping = 32 }`. Durations are in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(tag = "curve", rename_all = "snake_case", deny_unknown_fields)]
pub enum Animation {
	/// No animation: text jumps to its new place.
	None,

	/// Like a weight on a spring: pulled by `stiffness`, slowed by `damping`. With less damping than
	/// 2 × √stiffness it overshoots and bounces back.
	Spring {
		#[serde(default = "default_stiffness")]
		stiffness: f32,
		#[serde(default = "default_damping")]
		damping: f32,
	},

	Linear {
		#[serde(default = "default_duration")]
		duration: f32,
	},

	/// Fast at first, then slowing down
	EaseOut {
		#[serde(default = "default_duration")]
		duration: f32,
	},

	/// Speeding up, then slowing down
	EaseInOut {
		#[serde(default = "default_duration")]
		duration: f32,
	},

	/// Like CSS's `cubic-bezier()`: `points` are x1, y1, x2, y2, with the x values from 0 to 1
	CubicBezier {
		points: [f32; 4],
		#[serde(default = "default_duration")]
		duration: f32,
	},
}

fn default_stiffness() -> f32 {
	400.0
}

fn default_damping() -> f32 {
	32.0
}

fn default_duration() -> f32 {
	150.0
}

impl Default for Animation {
	fn default() -> Self {
		Self::Spring { stiffness: default_stiffness(), damping: default_damping() }
	}
}

/// How underlines are drawn, as adjustments to the font's own, in logical pixels
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnderlineSettings {
	/// Moves underlines down from the font's position, or up if negative
	pub offset: f32,

	/// Thickness of every line (underlines, overlines, strikethroughs), or `false` for the font's
	#[serde(deserialize_with = "number_or_false")]
	pub thickness: Option<f32>,

	/// Height of an undercurl's wave, in line thicknesses
	pub wave_height: f32,

	/// Space kept between underlines and the text's ink where it crosses them, or `false` to draw through it
	#[serde(deserialize_with = "number_or_false")]
	pub ink_gaps: Option<f32>,
}

impl Default for UnderlineSettings {
	fn default() -> Self {
		Self { offset: 0.0, thickness: None, wave_height: 2.0, ink_gaps: None }
	}
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TouchpadSettings {
	/// Lines scrolled per row of finger movement: 1 follows the fingers, more scrolls faster
	pub scroll_speed: f64,
}

impl Default for TouchpadSettings {
	fn default() -> Self {
		Self { scroll_speed: 3.0 }
	}
}

impl Settings {
	/// Whether the values make sense, beyond their types; the error names the first that doesn't
	pub fn validate(&self) -> Result<(), String> {
		let check = |valid: bool, message: &str| if valid { Ok(()) } else { Err(message.to_owned()) };
		match self.scroll.animation {
			Animation::None => (),
			Animation::Spring { stiffness, damping } => {
				check(stiffness > 0.0, "scroll.animation.stiffness must be more than 0")?;
				check(damping >= 0.0, "scroll.animation.damping can't be negative")?;
			},
			Animation::Linear { duration } | Animation::EaseOut { duration } | Animation::EaseInOut { duration } => {
				check(duration >= 0.0, "scroll.animation.duration can't be negative")?;
			},
			Animation::CubicBezier { points: [x1, _, x2, _], duration } => {
				check(
					(0.0..=1.0).contains(&x1) && (0.0..=1.0).contains(&x2),
					"scroll.animation.points: x1 and x2 must be from 0 to 1",
				)?;
				check(duration >= 0.0, "scroll.animation.duration can't be negative")?;
			},
		}

		let underline = &self.underline;
		check(underline.thickness.is_none_or(|thickness| thickness > 0.0), "underline.thickness must be more than 0")?;
		check(underline.wave_height > 0.0, "underline.wave_height must be more than 0")?;
		check(underline.ink_gaps.is_none_or(|clearance| clearance >= 0.0), "underline.ink_gaps can't be negative")?;
		check(self.touchpad.scroll_speed > 0.0, "touchpad.scroll_speed must be more than 0")?;
		Ok(())
	}
}

/// A number, or `false` for none (Lua can't keep `nil` in a table)
fn number_or_false<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f32>, D::Error> {
	struct NumberOrFalse;

	impl Visitor<'_> for NumberOrFalse {
		type Value = Option<f32>;

		fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
			formatter.write_str("a number or false")
		}

		fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
			if value { Err(E::invalid_value(de::Unexpected::Bool(true), &self)) } else { Ok(None) }
		}

		fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
			Ok(Some(value as f32))
		}

		fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
			Ok(Some(value as f32))
		}

		fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
			Ok(Some(value as f32))
		}
	}

	deserializer.deserialize_any(NumberOrFalse)
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::{Animation, Settings};

	/// A Lua table as Neovim sends it: a map, or an array when empty
	fn table(pairs: Vec<(&str, Value)>) -> Value {
		if pairs.is_empty() {
			return Value::Array(Vec::new());
		}
		Value::Map(pairs.into_iter().map(|(key, value)| (Value::from(key), value)).collect())
	}

	fn parse(value: Value) -> Result<Settings, String> {
		let settings: Settings = rmpv::ext::from_value(value).map_err(|error| error.to_string())?;
		settings.validate()?;
		Ok(settings)
	}

	#[test]
	fn nothing_given_keeps_the_defaults() {
		assert_eq!(parse(table(vec![])).unwrap(), Settings::default());
		assert_eq!(parse(table(vec![("scroll", table(vec![]))])).unwrap(), Settings::default());
	}

	#[test]
	fn values_given_replace_their_defaults_only() {
		let underline = table(vec![("ink_gaps", Value::from(1.5)), ("thickness", Value::from(2))]);
		let settings = parse(table(vec![("underline", underline)])).unwrap();
		assert_eq!((settings.underline.ink_gaps, settings.underline.thickness), (Some(1.5), Some(2.0)));
		assert_eq!(settings.underline.wave_height, 2.0);
		assert_eq!(settings.touchpad.scroll_speed, 3.0);

		// `false` turns a number off.
		let underline = table(vec![("ink_gaps", Value::from(false))]);
		assert_eq!(parse(table(vec![("underline", underline)])).unwrap().underline.ink_gaps, None);
	}

	#[test]
	fn curves() {
		let animation = |pairs| parse(table(vec![("scroll", table(vec![("animation", table(pairs))]))]));
		let curve = |name: &str| ("curve", Value::from(name));

		assert_eq!(animation(vec![curve("none")]).unwrap().scroll.animation, Animation::None);
		assert_eq!(
			animation(vec![curve("spring"), ("damping", Value::from(40))])
				.unwrap()
				.scroll
				.animation,
			Animation::Spring { stiffness: 400.0, damping: 40.0 },
		);
		assert_eq!(
			animation(vec![curve("ease_out")]).unwrap().scroll.animation,
			Animation::EaseOut { duration: 150.0 }
		);
		let points = Value::Array([0.25, 0.1, 0.25, 1.0].into_iter().map(Value::from).collect());
		let bezier =
			animation(vec![curve("cubic_bezier"), ("points", points), ("duration", Value::from(200))]).unwrap();
		assert_eq!(bezier.scroll.animation, Animation::CubicBezier { points: [0.25, 0.1, 0.25, 1.0], duration: 200.0 });
	}

	#[test]
	fn mistakes_are_named() {
		let error = parse(table(vec![("scrol", table(vec![]))])).unwrap_err();
		assert!(error.contains("unknown field `scrol`") && error.contains("`scroll`"), "{error}");

		let animation = |pairs| parse(table(vec![("scroll", table(vec![("animation", table(pairs))]))]));
		let error = animation(vec![("curve", Value::from("sprng"))]).unwrap_err();
		assert!(error.contains("unknown variant `sprng`") && error.contains("`spring`"), "{error}");
		// Another curve's value
		let error = animation(vec![("curve", Value::from("ease_out")), ("stiffness", Value::from(3))]).unwrap_err();
		assert!(error.contains("unknown field `stiffness`"), "{error}");

		let underline = |pairs| parse(table(vec![("underline", table(pairs))]));
		let error = underline(vec![("thickness", Value::from(true))]).unwrap_err();
		assert!(error.contains("a number or false"), "{error}");
		let error = underline(vec![("wave_height", Value::from(0))]).unwrap_err();
		assert_eq!(error, "underline.wave_height must be more than 0");
		let error = underline(vec![("offset", Value::from("down"))]).unwrap_err();
		assert!(error.contains("invalid type"), "{error}");
	}
}
