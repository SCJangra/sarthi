//! sarthi's settings, set from Lua with `require("sarthi").setup({ … })`. Each section and value is optional: what
//! isn't given keeps its default. Fonts aren't here: they're Neovim's own `guifont` and `linespace` options.
//!
//! The sections about a window can be set for one window too, in its `w:sarthi` table ([`WindowOptions`]): the same
//! sections with the same keys, each replacing the setting's value for that window, e.g.
//! `vim.w[win].sarthi = { shadow = { opacity = 0.6 } }`.

use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
	pub scroll: ScrollSettings,
	pub cursor: CursorSettings,
	pub shadow: ShadowSettings,
	pub outline: OutlineSettings,
	pub corners: CornerSettings,
	pub padding: PaddingSettings,
	pub underline: UnderlineSettings,
	pub touchpad: TouchpadSettings,
	pub window: WindowSettings,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollSettings {
	/// Whether text that scrolled slides to its new place, or jumps there
	pub enabled: bool,

	/// How it moves to its new place
	pub animation: Animation,

	/// Windows showing these filetypes jump to their new place, e.g. menus whose selected line is highlighted text,
	/// unless their `w:sarthi` gives `scroll.enabled`
	pub exclude_filetypes: Vec<String>,
}

impl Default for ScrollSettings {
	fn default() -> Self {
		Self { enabled: true, animation: Animation::default(), exclude_filetypes: Vec::new() }
	}
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

/// How the cursor moves to a new place: its corners each on their way, the leading ones faster, so it stretches towards
/// where it goes and shrinks back as it arrives
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CursorSettings {
	/// For moves farther than a couple of cells along a row
	pub animation: Animation,

	/// For short moves along a row, like typing
	pub short_animation: Animation,

	/// How much the trailing corners lag behind the leading ones: from 0, where the cursor moves as it is, to 1, where
	/// its leading edge is there at once
	pub trail: f32,

	/// Whether moves in insert and replace mode animate, e.g. while typing
	pub animate_insert_mode: bool,

	/// Whether moves into and out of the command line animate
	pub animate_cmdline: bool,
}

impl Default for CursorSettings {
	fn default() -> Self {
		Self {
			animation: Animation::Spring { stiffness: 700.0, damping: 53.0 },
			short_animation: Animation::EaseOut { duration: 40.0 },
			trail: 0.7,
			animate_insert_mode: true,
			animate_cmdline: true,
		}
	}
}

/// The shadow under floating windows, in logical pixels
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShadowSettings {
	pub enabled: bool,

	/// How far the edge fades out, like CSS's blur radius
	pub blur: f32,

	/// How far the shadow is moved from the window: right and down, or left and up if negative
	pub offset: [f32; 2],

	/// From 0, invisible, to 1, black where fully shadowed
	pub opacity: f32,

	/// Floating windows showing these filetypes have no shadow, e.g. scrollbars, unless their `w:sarthi` gives
	/// `shadow.enabled`
	pub exclude_filetypes: Vec<String>,
}

impl Default for ShadowSettings {
	fn default() -> Self {
		Self { enabled: true, blur: 16.0, offset: [0.0, 4.0], opacity: 0.3, exclude_filetypes: Vec::new() }
	}
}

/// A line around floating windows, just outside them, in logical pixels
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutlineSettings {
	/// Off by default: a window with a border of its own (`nvim_open_win()`'s `border`) would have two
	pub enabled: bool,

	pub width: f32,

	/// The highlight group whose text color the line has
	pub highlight: String,

	/// Floating windows showing these filetypes have no outline, unless their `w:sarthi` gives `outline.enabled`
	pub exclude_filetypes: Vec<String>,
}

impl Default for OutlineSettings {
	fn default() -> Self {
		Self { enabled: false, width: 1.0, highlight: String::from("FloatBorder"), exclude_filetypes: Vec::new() }
	}
}

/// Rounded corners of floating windows, in logical pixels
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CornerSettings {
	/// Off by default: a window's text in its corner cells isn't cut to them
	pub enabled: bool,

	pub radius: f32,

	/// Floating windows showing these filetypes keep square corners, unless their `w:sarthi` gives `corners.enabled`
	pub exclude_filetypes: Vec<String>,
}

impl Default for CornerSettings {
	fn default() -> Self {
		Self { enabled: false, radius: 8.0, exclude_filetypes: Vec::new() }
	}
}

/// Space around floating windows' cells, in their background, in logical pixels
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PaddingSettings {
	pub enabled: bool,

	pub size: f32,

	/// Floating windows showing these filetypes have no padding, unless their `w:sarthi` gives `padding.enabled`
	pub exclude_filetypes: Vec<String>,
}

impl Default for PaddingSettings {
	fn default() -> Self {
		Self { enabled: false, size: 4.0, exclude_filetypes: Vec::new() }
	}
}

/// The OS window: how see-through its background is, and whether the desktop behind it is blurred
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowSettings {
	/// How opaque the editor's background is, from 0 to 1. Text, highlighted backgrounds and floating windows stay
	/// opaque. Needs a compositor that shows windows' transparency.
	pub opacity: f32,

	/// Whether the desktop behind a see-through background is blurred. The compositor does it, where it can (e.g. KDE
	/// Plasma, macOS); elsewhere the background is only see-through.
	pub blur: bool,
}

impl Default for WindowSettings {
	fn default() -> Self {
		Self { opacity: 1.0, blur: false }
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

/// Declares `$options`, the values of settings section `$settings` that one window can have of its own: each optional,
/// left out keeping the setting's
macro_rules! window_options {
	($(#[$doc:meta])* $options:ident for $settings:ident { $($field:ident: $type:ty),* $(,)? }) => {
		$(#[$doc])*
		#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
		#[serde(default, deny_unknown_fields)]
		pub struct $options {
			$(pub $field: Option<$type>,)*
		}

		impl $settings {
			/// These settings, with the values `options` gives in their place
			pub fn with(&self, options: &$options) -> Self {
				let mut settings = self.clone();
				$(if let Some(value) = &options.$field {
					settings.$field = value.clone();
				})*
				settings
			}
		}
	};
}

window_options! {
	/// A window's own [`ScrollSettings`]
	ScrollOptions for ScrollSettings { enabled: bool, animation: Animation }
}

window_options! {
	/// A window's own [`ShadowSettings`]
	ShadowOptions for ShadowSettings { enabled: bool, blur: f32, offset: [f32; 2], opacity: f32 }
}

window_options! {
	/// A window's own [`OutlineSettings`]
	OutlineOptions for OutlineSettings { enabled: bool, width: f32, highlight: String }
}

window_options! {
	/// A window's own [`CornerSettings`]
	CornerOptions for CornerSettings { enabled: bool, radius: f32 }
}

window_options! {
	/// A window's own [`PaddingSettings`]
	PaddingOptions for PaddingSettings { enabled: bool, size: f32 }
}

/// What a window's `w:sarthi` sets for it: the sections of the settings about a window, with the same keys. Each value
/// given replaces the setting's for this window. `exclude_filetypes` is only in the settings: the window gives
/// `enabled` itself.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowOptions {
	pub scroll: ScrollOptions,
	pub shadow: ShadowOptions,
	pub outline: OutlineOptions,
	pub corners: CornerOptions,
	pub padding: PaddingOptions,
}

impl Settings {
	/// These settings with what `options` gives a window in their place, e.g. to validate them
	pub fn for_window(&self, options: &WindowOptions) -> Self {
		Self {
			scroll: self.scroll.with(&options.scroll),
			shadow: self.shadow.with(&options.shadow),
			outline: self.outline.with(&options.outline),
			corners: self.corners.with(&options.corners),
			padding: self.padding.with(&options.padding),
			..self.clone()
		}
	}

	/// Whether the values make sense, beyond their types; the error names the first that doesn't
	pub fn validate(&self) -> Result<(), String> {
		validate_animation("scroll.animation", self.scroll.animation)?;
		validate_animation("cursor.animation", self.cursor.animation)?;
		validate_animation("cursor.short_animation", self.cursor.short_animation)?;
		check((0.0..=1.0).contains(&self.cursor.trail), "cursor.trail must be from 0 to 1")?;
		check(self.shadow.blur >= 0.0, "shadow.blur can't be negative")?;
		check((0.0..=1.0).contains(&self.shadow.opacity), "shadow.opacity must be from 0 to 1")?;
		check(self.outline.width > 0.0, "outline.width must be more than 0")?;
		check(self.corners.radius >= 0.0, "corners.radius can't be negative")?;
		check(self.padding.size >= 0.0, "padding.size can't be negative")?;

		let underline = &self.underline;
		check(underline.thickness.is_none_or(|thickness| thickness > 0.0), "underline.thickness must be more than 0")?;
		check(underline.wave_height > 0.0, "underline.wave_height must be more than 0")?;
		check(underline.ink_gaps.is_none_or(|clearance| clearance >= 0.0), "underline.ink_gaps can't be negative")?;
		check(self.touchpad.scroll_speed > 0.0, "touchpad.scroll_speed must be more than 0")?;
		check((0.0..=1.0).contains(&self.window.opacity), "window.opacity must be from 0 to 1")?;
		Ok(())
	}
}

/// Whether `animation`'s values make sense; the error names the first that doesn't, as part of `name`
fn validate_animation(name: &str, animation: Animation) -> Result<(), String> {
	let check = |valid: bool, problem: &str| if valid { Ok(()) } else { Err(format!("{name}.{problem}")) };
	match animation {
		Animation::None => Ok(()),
		Animation::Spring { stiffness, damping } => {
			check(stiffness > 0.0, "stiffness must be more than 0")?;
			check(damping >= 0.0, "damping can't be negative")
		},
		Animation::Linear { duration } | Animation::EaseOut { duration } | Animation::EaseInOut { duration } => {
			check(duration >= 0.0, "duration can't be negative")
		},
		Animation::CubicBezier { points: [x1, _, x2, _], duration } => {
			let within = |x: f32| (0.0..=1.0).contains(&x);
			check(within(x1) && within(x2), "points: x1 and x2 must be from 0 to 1")?;
			check(duration >= 0.0, "duration can't be negative")
		},
	}
}

/// An error with `message`, unless `valid`
fn check(valid: bool, message: &str) -> Result<(), String> {
	if valid { Ok(()) } else { Err(message.to_owned()) }
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

	use super::{Animation, Settings, WindowOptions};

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

	#[test]
	fn shadow() {
		let shadow = |pairs| parse(table(vec![("shadow", table(pairs))]));
		let offset = Value::Array(vec![Value::from(2), Value::from(6)]);
		let settings = shadow(vec![("offset", offset), ("enabled", Value::from(false))])
			.unwrap()
			.shadow;
		assert_eq!((settings.offset, settings.enabled, settings.blur), ([2.0, 6.0], false, 16.0));

		let error = shadow(vec![("opacity", Value::from(1.5))]).unwrap_err();
		assert_eq!(error, "shadow.opacity must be from 0 to 1");
		let error = shadow(vec![("offset", Value::from(3))]).unwrap_err();
		assert!(error.contains("invalid type"), "{error}");
	}

	#[test]
	fn outline() {
		let outline = |pairs| parse(table(vec![("outline", table(pairs))]));
		let settings = outline(vec![("enabled", Value::from(true)), ("highlight", Value::from("Comment"))])
			.unwrap()
			.outline;
		assert_eq!((settings.enabled, settings.width, settings.highlight.as_str()), (true, 1.0, "Comment"));
		assert!(!Settings::default().outline.enabled);
		let error = outline(vec![("width", Value::from(0))]).unwrap_err();
		assert_eq!(error, "outline.width must be more than 0");
	}

	#[test]
	fn corners() {
		let corners = |pairs| parse(table(vec![("corners", table(pairs))]));
		let settings = corners(vec![("enabled", Value::from(true))]).unwrap().corners;
		assert_eq!((settings.enabled, settings.radius), (true, 8.0));
		assert!(!Settings::default().corners.enabled);
		let error = corners(vec![("radius", Value::from(-1))]).unwrap_err();
		assert_eq!(error, "corners.radius can't be negative");
	}

	#[test]
	fn padding() {
		let padding = |pairs| parse(table(vec![("padding", table(pairs))]));
		let settings = padding(vec![("enabled", Value::from(true)), ("size", Value::from(6))])
			.unwrap()
			.padding;
		assert_eq!((settings.enabled, settings.size), (true, 6.0));
		assert!(!Settings::default().padding.enabled);
		let error = padding(vec![("size", Value::from(-1))]).unwrap_err();
		assert_eq!(error, "padding.size can't be negative");
	}

	#[test]
	fn window_options() {
		// `w:sarthi`: the same sections and keys, each replacing the setting's
		let shadow = table(vec![("opacity", Value::from(0.6)), ("offset", Value::Array(vec![1.into(), 2.into()]))]);
		let scroll = table(vec![("enabled", Value::from(false))]);
		let options: WindowOptions =
			rmpv::ext::from_value(table(vec![("shadow", shadow), ("scroll", scroll)])).unwrap();
		let settings = Settings::default().for_window(&options);
		assert_eq!((settings.shadow.opacity, settings.shadow.offset, settings.shadow.blur), (0.6, [1.0, 2.0], 16.0));
		assert!(settings.shadow.enabled && !settings.scroll.enabled);
		assert_eq!(rmpv::ext::from_value::<WindowOptions>(table(vec![])).unwrap(), WindowOptions::default());

		// Only the sections about a window, without their filetypes: the window gives `enabled` itself
		let filetypes = table(vec![("exclude_filetypes", Value::Array(vec![]))]);
		assert!(rmpv::ext::from_value::<WindowOptions>(table(vec![("shadow", filetypes)])).is_err());
		assert!(rmpv::ext::from_value::<WindowOptions>(table(vec![("cursor", table(vec![]))])).is_err());
		let opacity = table(vec![("shadow", table(vec![("opacity", Value::from(2))]))]);
		let options: WindowOptions = rmpv::ext::from_value(opacity).unwrap();
		let error = Settings::default().for_window(&options).validate().unwrap_err();
		assert_eq!(error, "shadow.opacity must be from 0 to 1");

		// Every value of a section, not only whether it's on
		let outline = table(vec![("width", Value::from(3)), ("highlight", Value::from("Comment"))]);
		let options: WindowOptions = rmpv::ext::from_value(table(vec![("outline", outline)])).unwrap();
		let settings = Settings::default().for_window(&options);
		assert_eq!(
			(settings.outline.enabled, settings.outline.width, settings.outline.highlight.as_str()),
			(false, 3.0, "Comment")
		);
	}

	#[test]
	fn window() {
		let window = |pairs| parse(table(vec![("window", table(pairs))]));
		let settings = window(vec![("opacity", Value::from(0.85)), ("blur", Value::from(true))])
			.unwrap()
			.window;
		assert_eq!((settings.opacity, settings.blur), (0.85, true));
		assert_eq!(Settings::default().window.opacity, 1.0);
		let error = window(vec![("opacity", Value::from(1.5))]).unwrap_err();
		assert_eq!(error, "window.opacity must be from 0 to 1");
	}

	#[test]
	fn cursor() {
		let cursor = |pairs| parse(table(vec![("cursor", table(pairs))]));
		let settings = cursor(vec![("trail", Value::from(0.2)), ("animate_cmdline", Value::from(false))]).unwrap();
		assert_eq!((settings.cursor.trail, settings.cursor.animate_cmdline), (0.2, false));
		assert_eq!(settings.cursor.short_animation, Animation::EaseOut { duration: 40.0 });

		let error = cursor(vec![("trail", Value::from(2))]).unwrap_err();
		assert_eq!(error, "cursor.trail must be from 0 to 1");
		let linear = table(vec![("curve", Value::from("linear")), ("duration", Value::from(-1))]);
		let error = cursor(vec![("short_animation", linear)]).unwrap_err();
		assert_eq!(error, "cursor.short_animation.duration can't be negative");
	}
}
