//! sarthi's settings, set from Lua with `require("sarthi").setup({ … })`. Each section and value is optional: what
//! isn't given keeps its default. Fonts aren't here: they're Neovim's own `guifont` and `linespace` options.
//!
//! The sections about a window can be set for one window too, in its `w:sarthi` table ([`WindowOptions`]): the same
//! sections with the same keys, each replacing the setting's value for that window, e.g.
//! `vim.w[win].sarthi = { shadow = { opacity = 0.6 } }`.
//!
//! Each section is declared with [`section!`], which also keeps what the help file (`:help sarthi`) says about it: its
//! doc comments, its keys' types, and which keys a window can have its own values of. The help file's defaults are what
//! serde gives for an empty table.

use serde::{Deserialize, Serialize};

/// A key of a section of the settings, as declared, for the help file
#[derive(Debug)]
pub struct Key {
	pub name: &'static str,

	/// Its Rust type, as written
	pub rust_type: &'static str,

	/// Its Lua type where the Rust type doesn't say it, e.g. `number|false`; empty otherwise
	pub lua_type: &'static str,

	/// Its doc comment, line by line
	pub doc: &'static [&'static str],
}

/// A section of the settings, as declared with [`section!`], for the help file
pub trait Section {
	/// Its doc comment, line by line
	const DOC: &'static [&'static str];

	const KEYS: &'static [Key];

	/// The keys a window can have its own values of, in `w:sarthi`
	const WINDOW_KEYS: &'static [&'static str];
}

/// A section of the settings, by its name in them (see [`SECTIONS`])
#[derive(Debug)]
pub struct SectionDoc {
	pub name: &'static str,
	pub doc: &'static [&'static str],
	pub keys: &'static [Key],
	pub window_keys: &'static [&'static str],
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

/// Declares a section of the settings: a struct with each key's doc comment, type, Lua type if its Rust type doesn't say
/// it (`as "number|false"`), and default (`= …`), which serde gives for a key left out. `window $options { … }` names
/// the keys a window can have its own values of (see [`window_options!`]).
macro_rules! section {
	(
		$(#[doc = $doc:literal])*
		$name:ident $(, window $options:ident { $($window:ident: $window_type:ty),* $(,)? })? {
			$(
				$(#[doc = $key_doc:literal])*
				$(#[serde($($serde:tt)*)])?
				$key:ident: $type:ty $(as $lua:literal)? = $default:expr
			),* $(,)?
		}
	) => {
		$(#[doc = $doc])*
		#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
		#[serde(default, deny_unknown_fields)]
		pub struct $name {
			$(
				$(#[doc = $key_doc])*
				$(#[serde($($serde)*)])?
				pub $key: $type,
			)*
		}

		impl Default for $name {
			fn default() -> Self {
				Self { $($key: $default,)* }
			}
		}

		impl Section for $name {
			const DOC: &'static [&'static str] = &[$($doc),*];
			const KEYS: &'static [Key] = &[$(Key {
				name: stringify!($key),
				rust_type: stringify!($type),
				lua_type: concat!("" $(, $lua)?),
				doc: &[$($key_doc),*],
			}),*];
			const WINDOW_KEYS: &'static [&'static str] = &[$($(stringify!($window)),*)?];
		}

		$(window_options! {
			#[doc = concat!("A window's own [`", stringify!($name), "`]")]
			$options for $name { $($window: $window_type),* }
		})?
	};
}

/// Declares [`Settings`], with the sections in the order the help file lists them, and [`SECTIONS`]
macro_rules! settings {
	($($section:ident: $type:ident),* $(,)?) => {
		#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
		#[serde(default, deny_unknown_fields)]
		pub struct Settings {
			$(pub $section: $type,)*
		}

		/// Every section of the settings, in order, for the help file
		pub const SECTIONS: &[SectionDoc] = &[$(SectionDoc {
			name: stringify!($section),
			doc: <$type as Section>::DOC,
			keys: <$type as Section>::KEYS,
			window_keys: <$type as Section>::WINDOW_KEYS,
		}),*];
	};
}

settings! {
	scroll: ScrollSettings,
	cursor: CursorSettings,
	shadow: ShadowSettings,
	outline: OutlineSettings,
	corners: CornerSettings,
	padding: PaddingSettings,
	underline: UnderlineSettings,
	touchpad: TouchpadSettings,
	window: WindowSettings,
}

section! {
	/// How text that scrolled moves to its new place
	ScrollSettings, window ScrollOptions { enabled: bool, animation: Animation } {
		/// Whether text that scrolled slides to its new place, or jumps there
		enabled: bool = true,

		/// How it moves to its new place
		animation: Animation = Animation::default(),

		/// Windows showing these filetypes jump to their new place, e.g. menus whose selected line is highlighted text,
		/// unless their `w:sarthi` gives `scroll.enabled`
		exclude_filetypes: Vec<String> = Vec::new(),
	}
}

/// An animation's curve, e.g. `{ curve = "spring", stiffness = 400, damping = 32 }`, one of [`Animation::CURVES`].
/// Durations are in milliseconds; a value left out is serde's default.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(tag = "curve", rename_all = "snake_case", deny_unknown_fields)]
pub enum Animation {
	None,
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
	EaseOut {
		#[serde(default = "default_duration")]
		duration: f32,
	},
	EaseInOut {
		#[serde(default = "default_duration")]
		duration: f32,
	},
	CubicBezier {
		points: [f32; 4],
		#[serde(default = "default_duration")]
		duration: f32,
	},
}

/// A curve of [`Animation`], for the help file
#[derive(Debug)]
pub struct Curve {
	/// Its `curve` value
	pub name: &'static str,

	pub doc: &'static str,

	/// Its keys without a default, with example values, as Lua
	pub required: &'static str,
}

impl Animation {
	/// Every curve, described
	pub const CURVES: [Curve; 6] = [
		Curve { name: "none", doc: "No animation: it's in its new place at once.", required: "" },
		Curve {
			name: "spring",
			doc: "Like a weight on a spring: pulled by `stiffness`, slowed by `damping`. With less damping than \
			      2 × √stiffness it overshoots and bounces back.",
			required: "",
		},
		Curve { name: "linear", doc: "At an even speed over `duration`", required: "" },
		Curve { name: "ease_out", doc: "Fast at first, then slowing down, over `duration`", required: "" },
		Curve { name: "ease_in_out", doc: "Speeding up, then slowing down, over `duration`", required: "" },
		Curve {
			name: "cubic_bezier",
			doc: "Like CSS's `cubic-bezier()` over `duration`: `points` are x1, y1, x2, y2, with the x values from 0 to 1.",
			required: "points = { 0.25, 0.1, 0.25, 1 }",
		},
	];

	/// Its `curve` value, one of [`Animation::CURVES`]'
	pub fn curve(&self) -> &'static str {
		match self {
			Self::None => "none",
			Self::Spring { .. } => "spring",
			Self::Linear { .. } => "linear",
			Self::EaseOut { .. } => "ease_out",
			Self::EaseInOut { .. } => "ease_in_out",
			Self::CubicBezier { .. } => "cubic_bezier",
		}
	}
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

section! {
	/// How the cursor moves to a new place: its corners each on their way, the leading ones faster, so it stretches
	/// towards where it goes and shrinks back as it arrives
	CursorSettings {
		/// For moves farther than a couple of cells along a row
		animation: Animation = Animation::Spring { stiffness: 700.0, damping: 53.0 },

		/// For short moves along a row, like typing
		short_animation: Animation = Animation::EaseOut { duration: 40.0 },

		/// How much the trailing corners lag behind the leading ones: from 0, where the cursor moves as it is, to 1,
		/// where its leading edge is there at once
		trail: f32 = 0.7,

		/// Whether moves in insert and replace mode animate, e.g. while typing
		animate_insert_mode: bool = true,

		/// Whether moves into and out of the command line animate
		animate_cmdline: bool = true,
	}
}

section! {
	/// The shadow under floating and external windows, in logical pixels
	ShadowSettings, window ShadowOptions { enabled: bool, blur: f32, offset: [f32; 2], opacity: f32 } {
		/// Whether windows cast one
		enabled: bool = true,

		/// How far the edge fades out, like CSS's blur radius
		blur: f32 = 16.0,

		/// How far the shadow is moved from the window: right and down, or left and up if negative
		offset: [f32; 2] = [0.0, 4.0],

		/// From 0, invisible, to 1, black where fully shadowed
		opacity: f32 = 0.3,

		/// Windows showing these filetypes have no shadow, e.g. scrollbars, unless their `w:sarthi` gives
		/// `shadow.enabled`
		exclude_filetypes: Vec<String> = Vec::new(),
	}
}

section! {
	/// A line around floating and external windows, just outside them, in logical pixels
	OutlineSettings, window OutlineOptions { enabled: bool, width: f32, highlight: String } {
		/// Whether windows have one. Off by default: a window with a border of its own (`nvim_open_win()`'s `border`)
		/// would have two.
		enabled: bool = false,

		/// How thick the line is
		width: f32 = 1.0,

		/// The highlight group whose text color the line has
		highlight: String = String::from("FloatBorder"),

		/// Windows showing these filetypes have no outline, unless their `w:sarthi` gives `outline.enabled`
		exclude_filetypes: Vec<String> = Vec::new(),
	}
}

section! {
	/// Rounded corners of floating and external windows, in logical pixels. The outline follows them.
	CornerSettings, window CornerOptions { enabled: bool, radius: f32 } {
		/// Whether windows have them. Off by default: a window's text in its corner cells isn't cut to them.
		enabled: bool = false,

		radius: f32 = 8.0,

		/// Windows showing these filetypes keep square corners, unless their `w:sarthi` gives `corners.enabled`
		exclude_filetypes: Vec<String> = Vec::new(),
	}
}

section! {
	/// Space around floating and external windows' cells, in their background, in logical pixels
	PaddingSettings, window PaddingOptions { enabled: bool, size: PaddingSize } {
		/// Whether windows have it
		enabled: bool = false,

		/// In logical pixels: the same on every side, or each side's as numbers, those left out 0, e.g.
		/// `{ top = 2, left = 8, right = 8 }`
		size: PaddingSize as "number|{ top?, right?, bottom?, left? }" = PaddingSize::All(4.0),

		/// Windows showing these filetypes have no padding, unless their `w:sarthi` gives `padding.enabled`
		exclude_filetypes: Vec<String> = Vec::new(),
	}
}

section! {
	/// How underlines are drawn, as adjustments to the font's own, in logical pixels
	UnderlineSettings {
		/// Moves underlines down from the font's position, or up if negative
		offset: f32 = 0.0,

		/// Thickness of every line (underlines, overlines, strikethroughs), or `false` for the font's
		#[serde(with = "number_or_false")]
		thickness: Option<f32> as "number|false" = None,

		/// Height of an undercurl's wave, in line thicknesses
		wave_height: f32 = 2.0,

		/// Space kept between underlines and the text's ink where it crosses them, or `false` to draw through it
		#[serde(with = "number_or_false")]
		ink_gaps: Option<f32> as "number|false" = None,
	}
}

section! {
	/// Scrolling with a touchpad
	TouchpadSettings {
		/// Lines scrolled per row of finger movement: 1 follows the fingers, more scrolls faster
		scroll_speed: f64 = 3.0,
	}
}

section! {
	/// The OS window: how see-through its background is, and whether the desktop behind it is blurred
	WindowSettings {
		/// How opaque the editor's background is, from 0 to 1. Text, highlighted backgrounds and floating windows
		/// stay opaque. Needs a compositor that shows windows' transparency.
		opacity: f32 = 1.0,

		/// Whether the desktop behind a see-through background is blurred. The compositor does it, where it can (e.g.
		/// KDE Plasma, macOS); elsewhere the background is only see-through.
		blur: bool = false,
	}
}

/// The size of a window's padding: the same on every side, or each side's
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PaddingSize {
	All(f32),
	Sides(PaddingSides),
}

/// Each side's padding, in logical pixels; a side left out has none
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PaddingSides {
	pub top: f32,
	pub right: f32,
	pub bottom: f32,
	pub left: f32,
}

impl PaddingSize {
	/// Top, right, bottom and left
	pub fn sides(self) -> [f32; 4] {
		match self {
			Self::All(size) => [size; 4],
			Self::Sides(PaddingSides { top, right, bottom, left }) => [top, right, bottom, left],
		}
	}
}

impl<'de> Deserialize<'de> for PaddingSize {
	/// A number, or a table of sides: decoded as [`PaddingSides`], so a mistake in it is named
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		use serde::de::{self, value::MapAccessDeserializer};

		struct Size;

		impl<'de> de::Visitor<'de> for Size {
			type Value = PaddingSize;

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				formatter.write_str("a number, or a table of top, right, bottom and left")
			}

			fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
				Ok(PaddingSize::All(value as f32))
			}

			fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
				Ok(PaddingSize::All(value as f32))
			}

			fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
				Ok(PaddingSize::All(value as f32))
			}

			fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
				PaddingSides::deserialize(MapAccessDeserializer::new(map)).map(PaddingSize::Sides)
			}

			/// An empty Lua table arrives as an array: no padding on any side.
			fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
				match seq.next_element::<de::IgnoredAny>()? {
					None => Ok(PaddingSize::Sides(PaddingSides::default())),
					Some(_) => Err(de::Error::invalid_type(de::Unexpected::Seq, &self)),
				}
			}
		}

		deserializer.deserialize_any(Size)
	}
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
		check(self.padding.size.sides().iter().all(|size| *size >= 0.0), "padding.size can't be negative")?;

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
mod number_or_false {
	use serde::de::{self, Deserializer, Visitor};
	use serde::ser::Serializer;

	pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f32>, D::Error> {
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

	pub fn serialize<S: Serializer>(value: &Option<f32>, serializer: S) -> Result<S::Ok, S::Error> {
		match value {
			Some(number) => serializer.serialize_f32(*number),
			None => serializer.serialize_bool(false),
		}
	}
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
		assert_eq!((settings.enabled, settings.size.sides()), (true, [6.0; 4]));
		assert!(!Settings::default().padding.enabled);
		let error = padding(vec![("size", Value::from(-1))]).unwrap_err();
		assert_eq!(error, "padding.size can't be negative");

		// Each side its own, those left out none; an empty table, none at all
		let sides = table(vec![("top", Value::from(2)), ("left", Value::from(8.5))]);
		let size = padding(vec![("size", sides)]).unwrap().padding.size;
		assert_eq!(size.sides(), [2.0, 0.0, 0.0, 8.5]);
		assert_eq!(padding(vec![("size", table(vec![]))]).unwrap().padding.size.sides(), [0.0; 4]);
		let error = padding(vec![("size", table(vec![("lfet", Value::from(1))]))]).unwrap_err();
		assert!(error.contains("unknown field `lfet`"), "{error}");
		let error = padding(vec![("size", table(vec![("right", Value::from(-2))]))]).unwrap_err();
		assert_eq!(error, "padding.size can't be negative");
	}

	#[test]
	fn settings_go_to_lua_as_setup_takes_them() {
		// As `require("sarthi").settings()` gets them: every section and key, by name, which setup() takes back
		let mut settings = Settings::default();
		settings.padding.size = super::PaddingSize::Sides(super::PaddingSides { top: 2.0, ..Default::default() });
		settings.underline.ink_gaps = Some(1.5);
		let encoded = rmp_serde::to_vec_named(&settings).unwrap();
		assert_eq!(rmp_serde::from_slice::<Settings>(&encoded).unwrap(), settings);
		let encoded = rmp_serde::to_vec_named(&Settings::default()).unwrap();
		assert_eq!(rmp_serde::from_slice::<Settings>(&encoded).unwrap(), Settings::default());
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
