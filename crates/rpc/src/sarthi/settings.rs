//! sarthi's settings, set from Lua with `require("sarthi").setup({ … })`, and a window's own values of them
//! ([`WindowOptions`]).
//!
#![doc = include_str!("doc/settings.md")]
//!
//! Each section is a plain struct: its doc comments, types and defaults (`#[default(…)]`) are what the help file
//! (`:help sarthi`) and the Lua types say about it, through its schema ([`JsonSchema`]).

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;

use crate::{WindowFont, WindowPlace};

/// The settings `require('sarthi').setup()` takes, a section each, in the order the help file lists them
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
	/// How text that scrolled moves to its new place
	pub scroll: ScrollSettings,

	/// How the cursor moves to a new place
	pub cursor: CursorSettings,

	/// The shadow under floating and external windows
	pub shadow: ShadowSettings,

	/// A line around external windows
	pub outline: OutlineSettings,

	/// Rounded corners of external windows
	pub corners: CornerSettings,

	/// Space around external windows' cells
	pub padding: PaddingSettings,

	/// How underlines are drawn
	pub underline: UnderlineSettings,

	/// Scrolling with a touchpad
	pub touchpad: TouchpadSettings,

	/// The OS window
	pub window: WindowSettings,

	/// Typing through an input method
	pub ime: ImeSettings,
}

/// How text that scrolled moves to its new place
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollSettings {
	/// Whether text that scrolled slides to its new place, or jumps there
	#[default(true)]
	pub enabled: bool,

	/// How it moves to its new place
	pub animation: Animation,

	/// Windows showing these filetypes jump to their new place, e.g. menus whose selected line is highlighted text,
	/// whatever their `w:sarthi` gives. Glob patterns, e.g. `cmp_*`.
	pub exclude_filetypes: Filetypes,
}

/// A window's own `scroll` values (see [`ScrollSettings`])
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollOptions {
	/// Whether its text slides
	pub enabled: Option<bool>,

	/// How its text slides
	pub animation: Option<Animation>,
}

impl ScrollSettings {
	/// These settings, with the values `options` gives in their place
	pub fn with(&self, options: &ScrollOptions) -> Self {
		Self {
			enabled: options.enabled.unwrap_or(self.enabled),
			animation: options.animation.unwrap_or(self.animation),
			..self.clone()
		}
	}
}

/// An animation's curve, e.g. `{ curve = "spring", stiffness = 400, damping = 32 }`. Durations are in milliseconds; a
/// value left out has its default.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(tag = "curve", rename_all = "snake_case", deny_unknown_fields)]
pub enum Animation {
	/// No animation: it's in its new place at once
	None,

	/// Like a weight on a spring: pulled by `stiffness`, slowed by `damping`. With less damping than 2 × √stiffness it
	/// overshoots and bounces back.
	#[default]
	Spring {
		/// How hard the spring pulls
		#[serde(default = "default_stiffness")]
		#[default(default_stiffness())]
		stiffness: f32,

		/// How much the spring slows it down
		#[serde(default = "default_damping")]
		#[default(default_damping())]
		damping: f32,
	},

	/// At an even speed over `duration`
	Linear {
		/// How long it takes, in milliseconds
		#[serde(default = "default_duration")]
		duration: f32,
	},

	/// Fast at first, then slowing down, over `duration`
	EaseOut {
		/// How long it takes, in milliseconds
		#[serde(default = "default_duration")]
		duration: f32,
	},

	/// Speeding up, then slowing down, over `duration`
	EaseInOut {
		/// How long it takes, in milliseconds
		#[serde(default = "default_duration")]
		duration: f32,
	},

	/// Like CSS's `cubic-bezier()` over `duration`
	CubicBezier {
		/// The curve's control points: x1, y1, x2, y2, with the x values from 0 to 1, e.g. `{ 0.25, 0.1, 0.25, 1 }`
		points: [f32; 4],

		/// How long it takes, in milliseconds
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

/// How the cursor moves to a new place: its corners each on their way, the leading ones faster, so it stretches
/// towards where it goes and shrinks back as it arrives
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct CursorSettings {
	/// For moves farther than a couple of cells along a row
	#[default(Animation::Spring { stiffness: 700.0, damping: 53.0 })]
	pub animation: Animation,

	/// For short moves along a row, like typing
	#[default(Animation::EaseOut { duration: 40.0 })]
	pub short_animation: Animation,

	/// How much the trailing corners lag behind the leading ones: from 0, where the cursor moves as it is, to 1, where
	/// its leading edge is there at once
	#[default(1.0)]
	pub trail: f32,

	/// Whether moves in insert and replace mode animate, e.g. while typing
	#[default(true)]
	pub animate_insert_mode: bool,

	/// Whether moves into and out of the command line animate
	#[default(true)]
	pub animate_cmdline: bool,
}

/// The shadow under floating and external windows, in logical pixels
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct ShadowSettings {
	/// Whether windows cast one
	#[default(true)]
	pub enabled: bool,

	/// How far the edge fades out, like CSS's blur radius
	#[default(8.0)]
	pub blur: f32,

	/// How far the shadow is moved from the window: right and down, or left and up if negative
	#[default([0.0, 2.0])]
	pub offset: [f32; 2],

	/// From 0, invisible, to 1, black where fully shadowed
	#[default(1.0)]
	pub opacity: f32,

	/// Windows showing these filetypes have no shadow, e.g. scrollbars, whatever their `w:sarthi` gives. Glob patterns,
	/// e.g. `cmp_*`.
	pub exclude_filetypes: Filetypes,
}

/// A window's own `shadow` values (see [`ShadowSettings`])
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ShadowOptions {
	/// Whether it casts one
	pub enabled: Option<bool>,

	/// How far its shadow's edge fades out
	pub blur: Option<f32>,

	/// How far its shadow is moved
	pub offset: Option<[f32; 2]>,

	/// How dark its shadow is
	pub opacity: Option<f32>,
}

impl ShadowSettings {
	/// These settings, with the values `options` gives in their place
	pub fn with(&self, options: &ShadowOptions) -> Self {
		Self {
			enabled: options.enabled.unwrap_or(self.enabled),
			blur: options.blur.unwrap_or(self.blur),
			offset: options.offset.unwrap_or(self.offset),
			opacity: options.opacity.unwrap_or(self.opacity),
			..self.clone()
		}
	}
}

/// A line around external windows, just outside them, in logical pixels.
///
/// Not around floating windows, which Neovim places without room for it. For windows whose `w:sarthi` is `managed`,
/// e.g. opened with the [window functions](#sarthi.api); others have one only if their `w:sarthi` turns it on.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct OutlineSettings {
	/// Whether windows have one. Off by default: a window with a border of its own (`nvim_open_win()`'s `border`) would
	/// have two.
	pub enabled: bool,

	/// How thick the line is
	#[default(1.0)]
	pub width: f32,

	/// The highlight group whose text color the line has
	#[default(String::from("FloatBorder"))]
	pub highlight: String,

	/// Windows showing these filetypes have no outline, whatever their `w:sarthi` gives. Glob patterns, e.g. `cmp_*`.
	pub exclude_filetypes: Filetypes,
}

/// A window's own `outline` values (see [`OutlineSettings`])
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct OutlineOptions {
	/// Whether it has one
	pub enabled: Option<bool>,

	/// How thick its line is
	pub width: Option<f32>,

	/// The highlight group of its line
	pub highlight: Option<String>,
}

impl OutlineSettings {
	/// These settings, with the values `options` gives in their place
	pub fn with(&self, options: &OutlineOptions) -> Self {
		Self {
			enabled: options.enabled.unwrap_or(self.enabled),
			width: options.width.unwrap_or(self.width),
			highlight: options.highlight.clone().unwrap_or_else(|| self.highlight.clone()),
			..self.clone()
		}
	}
}

/// Rounded corners of external windows, in logical pixels.
///
/// Not of floating windows, whose text would be cut by them. The outline follows them. For windows whose `w:sarthi` is
/// `managed`, e.g. opened with the [window functions](#sarthi.api); others have them only if their `w:sarthi` turns
/// them on.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct CornerSettings {
	/// Whether windows have them. Off by default: a window's text in its corner cells isn't cut to them.
	pub enabled: bool,

	/// How round they are: the radius of their arcs, in logical pixels.
	#[default(8.0)]
	pub radius: f32,

	/// Windows showing these filetypes keep square corners, whatever their `w:sarthi` gives. Glob patterns, e.g.
	/// `cmp_*`.
	pub exclude_filetypes: Filetypes,
}

/// A window's own `corners` values (see [`CornerSettings`])
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CornerOptions {
	/// Whether it has them
	pub enabled: Option<bool>,

	/// How round they are
	pub radius: Option<f32>,
}

impl CornerSettings {
	/// These settings, with the values `options` gives in their place
	pub fn with(&self, options: &CornerOptions) -> Self {
		Self {
			enabled: options.enabled.unwrap_or(self.enabled),
			radius: options.radius.unwrap_or(self.radius),
			..self.clone()
		}
	}
}

/// Space around external windows' cells, in their background, in logical pixels.
///
/// Not around floating windows, which Neovim places without room for it. For windows whose `w:sarthi` is `managed`,
/// e.g. opened with the [window functions](#sarthi.api); others have it only if their `w:sarthi` turns it on.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct PaddingSettings {
	/// Whether windows have it
	pub enabled: bool,

	/// In logical pixels: the same on every side, or each side's as numbers, those left out 0, e.g.
	/// `{ top = 2, left = 8, right = 8 }`
	#[default(PaddingSize::All(4.0))]
	pub size: PaddingSize,

	/// Windows showing these filetypes have no padding, whatever their `w:sarthi` gives. Glob patterns, e.g. `cmp_*`.
	pub exclude_filetypes: Filetypes,
}

/// A window's own `padding` values (see [`PaddingSettings`])
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PaddingOptions {
	/// Whether it has it
	pub enabled: Option<bool>,

	/// How much it has
	pub size: Option<PaddingSize>,
}

impl PaddingSettings {
	/// These settings, with the values `options` gives in their place
	pub fn with(&self, options: &PaddingOptions) -> Self {
		Self {
			enabled: options.enabled.unwrap_or(self.enabled),
			size: options.size.unwrap_or(self.size),
			..self.clone()
		}
	}
}

/// How underlines are drawn, as adjustments to the font's own, in logical pixels
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct UnderlineSettings {
	/// Moves underlines down from the font's position, or up if negative
	pub offset: f32,

	/// Thickness of every line (underlines, overlines, strikethroughs), or `false` for the font's
	#[serde(with = "number_or_false")]
	#[schemars(with = "NumberOrFalse")]
	pub thickness: Option<f32>,

	/// Height of an undercurl's wave, in line thicknesses
	#[default(2.0)]
	pub wave_height: f32,

	/// Space kept between underlines and the text's ink where it crosses them, or `false` to draw through it
	#[serde(with = "number_or_false")]
	#[schemars(with = "NumberOrFalse")]
	pub ink_gaps: Option<f32>,
}

/// Scrolling with a touchpad
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct TouchpadSettings {
	/// Lines scrolled per row of finger movement: 1 follows the fingers, more scrolls faster
	#[default(3.0)]
	pub scroll_speed: f64,
}

/// Typing through an input method (IME), e.g. fcitx5 or IBus, for languages a keyboard can't type directly: the text
/// it composes shows underlined at the cursor until it's committed
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct ImeSettings {
	/// Whether text can be typed through an input method. Turned off, keys go to Neovim as typed: e.g. from an
	/// autocommand while in Normal mode, so the input method doesn't take its keys.
	#[default(true)]
	pub enabled: bool,
}

/// The OS window: how see-through its background is, and whether the desktop behind it is blurred
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct WindowSettings {
	/// How opaque the editor's background is, from 0 to 1. Text, highlighted backgrounds and floating windows stay
	/// opaque. Needs a compositor that shows windows' transparency; on X11, a compositing window manager, and even then
	/// some GPU drivers can't. Sarthi warns when it can't show it.
	#[default(1.0)]
	pub opacity: f32,

	/// Whether the desktop behind a see-through background is blurred. The compositor does it, where it can (e.g. KDE
	/// Plasma, macOS); elsewhere the background is only see-through. Never on X11, where Sarthi warns about it.
	pub blur: bool,
}

/// The size of a window's padding: the same on every side, or each side's
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
#[schemars(inline)]
pub enum PaddingSize {
	/// The same on every side
	All(f32),

	/// Each side's own
	Sides(PaddingSides),
}

/// Each side's padding, in logical pixels; a side left out has none
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PaddingSides {
	/// Above the cells
	pub top: f32,

	/// Right of the cells
	pub right: f32,

	/// Below the cells
	pub bottom: f32,

	/// Left of the cells
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

/// Filetypes a setting excludes, as glob patterns, e.g. `{ 'help', 'cmp_*' }`: compiled as the settings are decoded, so
/// a pattern that isn't one is a mistake in them
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filetypes(Vec<glob::Pattern>);

impl Filetypes {
	/// Compiles `patterns`, or says which one isn't a glob pattern
	///
	/// # Errors
	///
	/// Fails with the pattern that isn't a glob pattern.
	pub fn new<'a>(patterns: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
		patterns
			.into_iter()
			.map(|pattern| {
				glob::Pattern::new(pattern).map_err(|error| format!("`{pattern}` isn't a glob pattern: {error}"))
			})
			.collect::<Result<_, _>>()
			.map(Self)
	}

	/// Whether one of the patterns matches `filetype`
	pub fn matches(&self, filetype: &str) -> bool {
		self.0.iter().any(|pattern| pattern.matches(filetype))
	}
}

impl<'de> Deserialize<'de> for Filetypes {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let patterns = Vec::<String>::deserialize(deserializer)?;
		Self::new(patterns.iter().map(String::as_str)).map_err(serde::de::Error::custom)
	}
}

impl Serialize for Filetypes {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.collect_seq(self.0.iter().map(glob::Pattern::as_str))
	}
}

/// A list of glob patterns, as Lua gives them
impl JsonSchema for Filetypes {
	/// Written where it's used: it's short
	fn inline_schema() -> bool {
		true
	}

	fn schema_name() -> std::borrow::Cow<'static, str> {
		"Filetypes".into()
	}

	fn json_schema(_: &mut SchemaGenerator) -> Schema {
		json_schema!({ "type": "array", "items": { "type": "string" } })
	}
}

/// What a window's `w:sarthi` sets for it.
///
#[doc = include_str!("doc/window-options.md")]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WindowOptions {
	/// Whether the settings' `outline`, `corners` and `padding` apply to the window, under its own values, as to one
	/// opened with `require('sarthi').api`, which sets it. Without it, the window has only what it gives of those; the
	/// settings' `scroll` and `shadow` apply to every window.
	pub managed: Option<bool>,
	/// The window's own `scroll`
	pub scroll: ScrollOptions,
	/// The window's own `shadow`
	pub shadow: ShadowOptions,
	/// The window's own `outline`
	pub outline: OutlineOptions,
	/// The window's own `corners`
	pub corners: CornerOptions,
	/// The window's own `padding`
	pub padding: PaddingOptions,
}

/// The `sarthi` table of a window's config, which the [window functions](#sarthi.api) take.
///
/// The window's own values of the settings' sections about a window, kept in its `w:sarthi`, and an external window's
/// place and fonts.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WindowConfig {
	/// Where an external window goes. External windows only.
	pub place: Option<WindowPlace>,

	/// The fonts an external window's text is drawn with. External windows only.
	pub font: Option<WindowFont>,

	/// Its own [shadow](#sarthi-shadow) values
	pub shadow: Option<ShadowOptions>,

	/// Its own [outline](#sarthi-outline) values. External windows only.
	pub outline: Option<OutlineOptions>,

	/// Its own [corners](#sarthi-corners) values. External windows only.
	pub corners: Option<CornerOptions>,

	/// Its own [padding](#sarthi-padding) values. External windows only.
	pub padding: Option<PaddingOptions>,

	/// Its own [scroll](#sarthi-scroll) values
	pub scroll: Option<ScrollOptions>,
}

impl Settings {
	/// The settings a window has with `options` from its `w:sarthi`: its own values, and for those it doesn't give, the
	/// settings' `scroll` and `shadow`, and their `outline`, `corners` and `padding` if it's managed, or else those
	/// sections' defaults. The filetypes each excludes are the settings'.
	pub fn for_window(&self, options: &WindowOptions) -> Self {
		let managed = options.managed.unwrap_or(false);
		let outline = if managed {
			self.outline.clone()
		} else {
			OutlineSettings { exclude_filetypes: self.outline.exclude_filetypes.clone(), ..Default::default() }
		};
		let corners = if managed {
			self.corners.clone()
		} else {
			CornerSettings { exclude_filetypes: self.corners.exclude_filetypes.clone(), ..Default::default() }
		};
		let padding = if managed {
			self.padding.clone()
		} else {
			PaddingSettings { exclude_filetypes: self.padding.exclude_filetypes.clone(), ..Default::default() }
		};
		Self {
			scroll: self.scroll.with(&options.scroll),
			shadow: self.shadow.with(&options.shadow),
			outline: outline.with(&options.outline),
			corners: corners.with(&options.corners),
			padding: padding.with(&options.padding),
			..self.clone()
		}
	}

	/// Whether the values make sense, beyond their types; the error names the first that doesn't
	///
	/// # Errors
	///
	/// Fails with what doesn't make sense, e.g. a negative size.
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

/// The schema of a number or `false`, for the values [`number_or_false`] decodes
struct NumberOrFalse;

impl JsonSchema for NumberOrFalse {
	/// Written where it's used: it's short
	fn inline_schema() -> bool {
		true
	}

	fn schema_name() -> std::borrow::Cow<'static, str> {
		"NumberOrFalse".into()
	}

	fn json_schema(_: &mut SchemaGenerator) -> Schema {
		json_schema!({ "anyOf": [{ "type": "number" }, { "const": false }] })
	}
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

	use super::{Animation, CornerOptions, Filetypes, Settings, WindowOptions};

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
		assert_eq!((settings.offset, settings.enabled, settings.blur), ([2.0, 6.0], false, 8.0));

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
		assert_eq!((settings.shadow.opacity, settings.shadow.offset, settings.shadow.blur), (0.6, [1.0, 2.0], 8.0));
		assert!(settings.shadow.enabled && !settings.scroll.enabled);
		assert_eq!(rmpv::ext::from_value::<WindowOptions>(table(vec![])).unwrap(), WindowOptions::default());

		// Only the sections about a window, without their filetypes: those are only the settings'
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
	fn managed_windows_have_the_settings_frames() {
		let mut settings = Settings::default();
		settings.shadow.opacity = 0.5;
		settings.outline.enabled = true;
		settings.outline.width = 2.0;
		settings.corners.enabled = true;
		settings.padding.enabled = true;
		settings.padding.exclude_filetypes = Filetypes::new(["help"]).unwrap();
		let radius =
			WindowOptions { corners: CornerOptions { radius: Some(3.0), ..Default::default() }, ..Default::default() };

		// Not managed: the settings' shadow, but only the window's own outline, corners and padding over the defaults
		let window = settings.for_window(&radius);
		assert_eq!(window.shadow.opacity, 0.5);
		assert_eq!((window.outline.enabled, window.outline.width), (false, 1.0));
		assert_eq!((window.corners.enabled, window.corners.radius), (false, 3.0));
		assert!(!window.padding.enabled);
		// The settings' filetypes, whichever
		assert!(window.padding.exclude_filetypes.matches("help"));

		// Managed: the settings', under the window's own
		let managed = WindowOptions { managed: Some(true), ..radius };
		let window = settings.for_window(&managed);
		assert_eq!((window.outline.enabled, window.outline.width), (true, 2.0));
		assert_eq!((window.corners.enabled, window.corners.radius), (true, 3.0));
		assert!(window.padding.enabled && window.padding.exclude_filetypes.matches("help"));

		let options: WindowOptions = rmpv::ext::from_value(table(vec![("managed", Value::from(true))])).unwrap();
		assert_eq!(options.managed, Some(true));
	}

	#[test]
	fn filetypes_are_glob_patterns() {
		let shadow = |filetypes: Vec<&str>| {
			let filetypes = Value::Array(filetypes.into_iter().map(Value::from).collect());
			parse(table(vec![("shadow", table(vec![("exclude_filetypes", filetypes)]))]))
		};
		let filetypes = shadow(vec!["help", "cmp_*", "dap-repl?"]).unwrap().shadow.exclude_filetypes;
		assert!(filetypes.matches("help") && filetypes.matches("cmp_menu") && filetypes.matches("dap-repl2"));
		assert!(!filetypes.matches("helpx") && !filetypes.matches("lua") && !filetypes.matches("cmp"));
		assert!(!Filetypes::default().matches(""));
		assert_eq!(shadow(vec![]).unwrap().shadow.exclude_filetypes, Filetypes::default());

		let error = shadow(vec!["[help"]).unwrap_err();
		assert!(error.contains("`[help` isn't a glob pattern"), "{error}");

		// Back to Lua as given
		let encoded = rmp_serde::to_vec_named(&shadow(vec!["cmp_*"]).unwrap()).unwrap();
		assert_eq!(
			rmp_serde::from_slice::<Settings>(&encoded).unwrap().shadow.exclude_filetypes,
			Filetypes::new(["cmp_*"]).unwrap()
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
