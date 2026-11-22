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
	/// Neovim's windows: how their text scrolls and what's drawn around them. Each window can have its own values in
	/// its `w:sarthi`.
	pub window: WindowSettings,

	/// How the cursor moves to a new place
	pub cursor_animation: CursorAnimationSettings,

	/// How underlines are drawn
	pub underline: UnderlineSettings,

	/// Scrolling with a touchpad
	pub touchpad: TouchpadSettings,

	/// Typing through an input method
	pub ime: ImeSettings,

	/// The editor's background in the OS window
	pub background: BackgroundSettings,
}

/// Neovim's windows: how their text scrolls and what's drawn around them. Each window can have its own values in its
/// `w:sarthi`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WindowSettings {
	/// How text that scrolled moves to its new place
	pub scroll_animation: ScrollAnimationSettings,

	/// The shadow under floating and external windows
	pub shadow: ShadowSettings,

	/// A line around external windows
	pub outline: OutlineSettings,

	/// Rounded corners of external windows
	pub rounded_corners: RoundedCornerSettings,

	/// Space around external windows' cells
	pub padding: PaddingSettings,
}

/// How text that scrolled moves to its new place
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollAnimationSettings {
	/// Whether text that scrolled slides to its new place, or jumps there
	#[default(true)]
	pub enabled: bool,

	/// How it moves to its new place
	pub curve: Animation,

	/// Other values for some scrolls ([overrides](#sarthi.ScrollOverride)): each one that applies to a scroll in place
	/// of `curve`, later ones winning, e.g. `{ { lines = { min = 40 }, curve = { type = 'none' } } }` for big jumps
	pub overrides: Vec<ScrollOverride>,

	/// Windows showing these filetypes jump to their new place, e.g. menus whose selected line is highlighted text,
	/// whatever their `w:sarthi` gives. Glob patterns, e.g. `cmp_*`.
	pub exclude_filetypes: Filetypes,
}

/// A value of `scroll_animation` for the scrolls it applies to
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollOverride {
	/// For scrolls of this many lines: a number, or `{ min = …, max = … }`, either left out for no bound
	#[serde(skip_serializing_if = "Span::is_any")]
	pub lines: Span,

	/// How text that scrolled that far moves to its new place
	#[serde(skip_serializing_if = "Option::is_none")]
	pub curve: Option<Animation>,
}

/// A window's own `scroll_animation` values (see [`ScrollAnimationSettings`])
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollAnimationOptions {
	/// Whether its text slides
	pub enabled: Option<bool>,

	/// How its text slides
	pub curve: Option<Animation>,
}

impl ScrollAnimationSettings {
	/// These settings, with the values `options` gives in their place
	pub fn with(&self, options: &ScrollAnimationOptions) -> Self {
		Self {
			enabled: options.enabled.unwrap_or(self.enabled),
			curve: options.curve.unwrap_or(self.curve),
			..self.clone()
		}
	}

	/// The curve of a scroll of `lines` lines: `curve`, or that of the last override that applies to it
	pub fn curve_for(&self, lines: u32) -> Animation {
		self.overrides
			.iter()
			.filter(|rule| rule.lines.contains(lines))
			.fold(self.curve, |curve, rule| rule.curve.unwrap_or(curve))
	}
}

/// An animation's curve, e.g. `{ type = "spring", stiffness = 400, damping = 32 }`. Durations are in milliseconds; a
/// value left out has its default.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
pub struct CursorAnimationSettings {
	/// Whether the cursor moves to its new place, or jumps there
	#[default(true)]
	pub enabled: bool,

	/// How it moves to its new place
	#[default(Animation::Spring { stiffness: 700.0, damping: 53.0 })]
	pub curve: Animation,

	/// How much the trailing corners lag behind the leading ones: from 0, where the cursor moves as it is, to 1, where
	/// its leading edge is there at once
	#[default(1.0)]
	pub trail: f32,

	/// Other values for some moves ([overrides](#sarthi.CursorOverride)): each one that applies to a move in place of
	/// `curve` and `trail`, later ones winning. By default, short moves along a row, like typing, are quick.
	#[default(vec![CursorOverride {
		rows: Span::exactly(0),
		columns: Span { min: None, max: Some(2) },
		curve: Some(Animation::EaseOut { duration: 40.0 }),
		..Default::default()
	}])]
	pub overrides: Vec<CursorOverride>,
}

/// Values of `cursor_animation` for the moves it applies to, e.g. `{ modes = 'i', curve = { type = 'none' } }`: all
/// its conditions must hold, and those it leaves out always do
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CursorOverride {
	/// For moves across this many rows: a number, or `{ min = …, max = … }`, either left out for no bound
	#[serde(skip_serializing_if = "Span::is_any")]
	pub rows: Span,

	/// For moves across this many columns, like `rows`
	#[serde(skip_serializing_if = "Span::is_any")]
	pub columns: Span,

	/// For moves within these modes, as `vim.keymap.set()` names them: one, or a list, of `n`, `v` (Visual and
	/// Select), `o`, `i` (Insert and Replace), `c` and `t`. Not for moves into or out of them, e.g. `A` from normal
	/// mode, or `:`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub modes: Option<Modes>,

	/// How the cursor moves to its new place
	#[serde(skip_serializing_if = "Option::is_none")]
	pub curve: Option<Animation>,

	/// How much the trailing corners lag behind the leading ones
	#[serde(skip_serializing_if = "Option::is_none")]
	pub trail: Option<f32>,
}

/// A move of the cursor, as overrides see it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorMove {
	/// Rows from where it was, in cells of where it goes
	pub rows: u32,

	/// Columns from where it was, in cells of where it goes
	pub columns: u32,

	/// The mode it moves within; none for a move into another mode
	pub mode: Option<Mode>,
}

/// How the cursor makes a move: the values of `cursor_animation` for it
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorStep {
	/// How it moves to its new place
	pub curve: Animation,

	/// How much the trailing corners lag behind the leading ones
	pub trail: f32,
}

impl CursorOverride {
	/// Whether it applies to `step`
	pub fn applies_to(&self, step: &CursorMove) -> bool {
		self.rows.contains(step.rows)
			&& self.columns.contains(step.columns)
			&& self
				.modes
				.as_ref()
				.is_none_or(|modes| step.mode.is_some_and(|mode| modes.0.contains(&mode)))
	}
}

impl CursorAnimationSettings {
	/// How the cursor makes `step`: `curve` and `trail`, with each override that applies to it in order, later ones
	/// winning
	pub fn for_move(&self, step: &CursorMove) -> CursorStep {
		self.overrides.iter().filter(|rule| rule.applies_to(step)).fold(
			CursorStep { curve: self.curve, trail: self.trail },
			|current, rule| CursorStep {
				curve: rule.curve.unwrap_or(current.curve),
				trail: rule.trail.unwrap_or(current.trail),
			},
		)
	}
}

/// How many of something a rule applies to: exactly a number, or from `min` to `max`, either left out for no bound
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
	/// The fewest it applies to
	pub min: Option<u32>,

	/// The most it applies to
	pub max: Option<u32>,
}

impl Span {
	/// Only `count`
	pub fn exactly(count: u32) -> Self {
		Self { min: Some(count), max: Some(count) }
	}

	/// Whether it applies to `count`
	pub fn contains(&self, count: u32) -> bool {
		self.min.is_none_or(|min| count >= min) && self.max.is_none_or(|max| count <= max)
	}

	/// Whether it applies to any count, having no bounds
	pub fn is_any(&self) -> bool {
		*self == Self::default()
	}
}

impl<'de> Deserialize<'de> for Span {
	/// A number, or a table of `min` and `max`: decoded as fields, so a mistake in it is named
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		use serde::de::{self, value::MapAccessDeserializer};

		#[derive(Default, Deserialize)]
		#[serde(default, deny_unknown_fields)]
		struct Bounds {
			min: Option<u32>,
			max: Option<u32>,
		}

		struct NumberOrBounds;

		impl<'de> de::Visitor<'de> for NumberOrBounds {
			type Value = Span;

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				formatter.write_str("a number, or a table of min and max")
			}

			fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
				u32::try_from(value)
					.map(Span::exactly)
					.map_err(|_| E::invalid_value(de::Unexpected::Unsigned(value), &self))
			}

			fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
				u64::try_from(value)
					.map_err(|_| E::invalid_value(de::Unexpected::Signed(value), &self))
					.and_then(|value| self.visit_u64(value))
			}

			fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
				let Bounds { min, max } = Bounds::deserialize(MapAccessDeserializer::new(map))?;
				Ok(Span { min, max })
			}

			/// An empty Lua table arrives as an array: no bounds.
			fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
				match seq.next_element::<de::IgnoredAny>()? {
					None => Ok(Span::default()),
					Some(_) => Err(de::Error::invalid_type(de::Unexpected::Seq, &self)),
				}
			}
		}

		deserializer.deserialize_any(NumberOrBounds)
	}
}

impl Serialize for Span {
	/// As `setup()` takes it: a number for exactly one count, otherwise a table of the bounds it has
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		use serde::ser::SerializeMap;

		if let (Some(min), Some(max)) = (self.min, self.max)
			&& min == max
		{
			return serializer.serialize_u32(min);
		}
		let bounds = [("min", self.min), ("max", self.max)];
		let mut map = serializer.serialize_map(None)?;
		for (key, bound) in bounds.iter().filter_map(|(key, bound)| Some((key, (*bound)?))) {
			map.serialize_entry(key, &bound)?;
		}
		map.end()
	}
}

/// A number, or a table of `min` and `max`, as Lua gives them
impl JsonSchema for Span {
	/// Written where it's used: it's short
	fn inline_schema() -> bool {
		true
	}

	fn schema_name() -> std::borrow::Cow<'static, str> {
		"Span".into()
	}

	fn json_schema(_: &mut SchemaGenerator) -> Schema {
		let count = json_schema!({ "type": "integer", "minimum": 0 });
		let bounds = json_schema!({ "type": "object", "properties": { "min": count, "max": count } });
		json_schema!({ "anyOf": [count, bounds], "x-lua-type": "integer|{ min: integer?, max: integer? }" })
	}
}

/// A mode, by the short name `vim.keymap.set()` gives it. Neovim tells Visual and Select mode apart only by the
/// cursor's look, so `v` is both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum Mode {
	/// Normal mode
	#[serde(rename = "n")]
	Normal,

	/// Visual and Select mode
	#[serde(rename = "v")]
	Visual,

	/// Operator-pending mode, e.g. after `d`
	#[serde(rename = "o")]
	OperatorPending,

	/// Insert and Replace mode
	#[serde(rename = "i")]
	Insert,

	/// The command line, e.g. after `:` or `/`
	#[serde(rename = "c")]
	Cmdline,

	/// Terminal mode
	#[serde(rename = "t")]
	Terminal,
}

/// Modes, as `vim.keymap.set()` takes them: one, e.g. `'i'`, or a list, e.g. `{ 'i', 'c' }`
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Modes(pub Vec<Mode>);

impl<'de> Deserialize<'de> for Modes {
	/// A mode or a list of them, each a string decoded as a [`Mode`], so a mistake in it is named: msgpack decoders
	/// take an enum as a map or an index, not as its name
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		use serde::de::{self, IntoDeserializer, value::SeqAccessDeserializer};

		fn mode<E: de::Error>(name: &str) -> Result<Mode, E> {
			Mode::deserialize(name.into_deserializer())
		}

		struct OneOrList;

		impl<'de> de::Visitor<'de> for OneOrList {
			type Value = Modes;

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				formatter.write_str("a mode, or a list of modes")
			}

			fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
				mode(value).map(|mode| Modes(vec![mode]))
			}

			fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
				let names = Vec::<String>::deserialize(SeqAccessDeserializer::new(seq))?;
				names.iter().map(|name| mode(name)).collect::<Result<_, _>>().map(Modes)
			}
		}

		deserializer.deserialize_any(OneOrList)
	}
}

/// A mode or a list of them, as Lua gives them
impl JsonSchema for Modes {
	/// Written where it's used: it's short
	fn inline_schema() -> bool {
		true
	}

	fn schema_name() -> std::borrow::Cow<'static, str> {
		"Modes".into()
	}

	fn json_schema(generator: &mut SchemaGenerator) -> Schema {
		let mode = generator.subschema_for::<Mode>();
		json_schema!({ "anyOf": [mode, { "type": "array", "items": mode }] })
	}
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
pub struct RoundedCornerSettings {
	/// Whether windows have them. Off by default: a window's text in its corner cells isn't cut to them.
	pub enabled: bool,

	/// How round they are: the radius of their arcs, in logical pixels.
	#[default(8.0)]
	pub radius: f32,

	/// Windows showing these filetypes keep square corners, whatever their `w:sarthi` gives. Glob patterns, e.g.
	/// `cmp_*`.
	pub exclude_filetypes: Filetypes,
}

/// A window's own `rounded_corners` values (see [`RoundedCornerSettings`])
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RoundedCornerOptions {
	/// Whether it has them
	pub enabled: Option<bool>,

	/// How round they are
	pub radius: Option<f32>,
}

impl RoundedCornerSettings {
	/// These settings, with the values `options` gives in their place
	pub fn with(&self, options: &RoundedCornerOptions) -> Self {
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

/// The editor's background in the OS window: how see-through it is, and whether the desktop behind it is blurred
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct BackgroundSettings {
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
	/// Whether the settings' `outline`, `rounded_corners` and `padding` apply to the window, under its own values, as
	/// to one opened with `require('sarthi').api`, which sets it. Without it, the window has only what it gives of
	/// those; the settings' `scroll_animation` and `shadow` apply to every window.
	pub managed: Option<bool>,
	/// The window's own `scroll_animation`
	pub scroll_animation: ScrollAnimationOptions,
	/// The window's own `shadow`
	pub shadow: ShadowOptions,
	/// The window's own `outline`
	pub outline: OutlineOptions,
	/// The window's own `rounded_corners`
	pub rounded_corners: RoundedCornerOptions,
	/// The window's own `padding`
	pub padding: PaddingOptions,
}

/// The `sarthi` table of a window's config, which the [window functions](#sarthi.api) take.
///
/// The window's own values of the settings' `window` sections, kept in its `w:sarthi`, and an external window's place
/// and fonts.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WindowConfig {
	/// Where an external window goes. External windows only.
	pub place: Option<WindowPlace>,

	/// The fonts an external window's text is drawn with. External windows only.
	pub font: Option<WindowFont>,

	/// Its own [shadow](#sarthi-window.shadow) values
	pub shadow: Option<ShadowOptions>,

	/// Its own [outline](#sarthi-window.outline) values. External windows only.
	pub outline: Option<OutlineOptions>,

	/// Its own [rounded corners](#sarthi-window.rounded_corners) values. External windows only.
	pub rounded_corners: Option<RoundedCornerOptions>,

	/// Its own [padding](#sarthi-window.padding) values. External windows only.
	pub padding: Option<PaddingOptions>,

	/// Its own [scroll animation](#sarthi-window.scroll_animation) values
	pub scroll_animation: Option<ScrollAnimationOptions>,
}

impl WindowSettings {
	/// The settings a window has with `options` from its `w:sarthi`: its own values, and for those it doesn't give,
	/// these `scroll_animation` and `shadow`, and these `outline`, `rounded_corners` and `padding` if it's managed, or
	/// else those sections' defaults. The filetypes each excludes are these settings'.
	pub fn with(&self, options: &WindowOptions) -> Self {
		let managed = options.managed.unwrap_or(false);
		let outline = if managed {
			self.outline.clone()
		} else {
			OutlineSettings { exclude_filetypes: self.outline.exclude_filetypes.clone(), ..Default::default() }
		};
		let rounded_corners = if managed {
			self.rounded_corners.clone()
		} else {
			let exclude_filetypes = self.rounded_corners.exclude_filetypes.clone();
			RoundedCornerSettings { exclude_filetypes, ..Default::default() }
		};
		let padding = if managed {
			self.padding.clone()
		} else {
			PaddingSettings { exclude_filetypes: self.padding.exclude_filetypes.clone(), ..Default::default() }
		};
		Self {
			scroll_animation: self.scroll_animation.with(&options.scroll_animation),
			shadow: self.shadow.with(&options.shadow),
			outline: outline.with(&options.outline),
			rounded_corners: rounded_corners.with(&options.rounded_corners),
			padding: padding.with(&options.padding),
		}
	}

	/// Whether the values make sense, beyond their types; the error names the first that doesn't by its key in a
	/// window's `w:sarthi`, e.g. `shadow.opacity`
	///
	/// # Errors
	///
	/// Fails with what doesn't make sense, e.g. a negative size.
	pub fn validate(&self) -> Result<(), String> {
		validate_animation("scroll_animation.curve", self.scroll_animation.curve)?;
		for (name, rule) in numbered("scroll_animation.overrides", &self.scroll_animation.overrides) {
			validate_span(&format!("{name}.lines"), rule.lines)?;
			let Some(curve) = rule.curve else {
				return Err(format!("{name} changes nothing: give it a curve"));
			};
			validate_animation(&format!("{name}.curve"), curve)?;
		}
		check(self.shadow.blur >= 0.0, "shadow.blur can't be negative")?;
		check((0.0..=1.0).contains(&self.shadow.opacity), "shadow.opacity must be from 0 to 1")?;
		check(self.outline.width > 0.0, "outline.width must be more than 0")?;
		check(self.rounded_corners.radius >= 0.0, "rounded_corners.radius can't be negative")?;
		check(self.padding.size.sides().iter().all(|size| *size >= 0.0), "padding.size can't be negative")
	}
}

impl Settings {
	/// Whether the values make sense, beyond their types; the error names the first that doesn't
	///
	/// # Errors
	///
	/// Fails with what doesn't make sense, e.g. a negative size.
	pub fn validate(&self) -> Result<(), String> {
		self.window.validate().map_err(|error| format!("window.{error}"))?;

		let cursor = &self.cursor_animation;
		validate_animation("cursor_animation.curve", cursor.curve)?;
		check((0.0..=1.0).contains(&cursor.trail), "cursor_animation.trail must be from 0 to 1")?;
		for (name, rule) in numbered("cursor_animation.overrides", &cursor.overrides) {
			validate_span(&format!("{name}.rows"), rule.rows)?;
			validate_span(&format!("{name}.columns"), rule.columns)?;
			if let Some(curve) = rule.curve {
				validate_animation(&format!("{name}.curve"), curve)?;
			}
			let trail = rule.trail.is_none_or(|trail| (0.0..=1.0).contains(&trail));
			check(trail, &format!("{name}.trail must be from 0 to 1"))?;
			let changes = rule.curve.is_some() || rule.trail.is_some();
			check(changes, &format!("{name} changes nothing: give it a curve or a trail"))?;
		}

		let underline = &self.underline;
		check(underline.thickness.is_none_or(|thickness| thickness > 0.0), "underline.thickness must be more than 0")?;
		check(underline.wave_height > 0.0, "underline.wave_height must be more than 0")?;
		check(underline.ink_gaps.is_none_or(|clearance| clearance >= 0.0), "underline.ink_gaps can't be negative")?;
		check(self.touchpad.scroll_speed > 0.0, "touchpad.scroll_speed must be more than 0")?;
		check((0.0..=1.0).contains(&self.background.opacity), "background.opacity must be from 0 to 1")?;
		Ok(())
	}
}

/// The items of list `name`, each with its name as Lua numbers it, e.g. `overrides[1]`
fn numbered<'a, T>(name: &'a str, items: &'a [T]) -> impl Iterator<Item = (String, &'a T)> {
	items
		.iter()
		.enumerate()
		.map(move |(index, item)| (format!("{name}[{}]", index + 1), item))
}

/// Whether `span` makes sense: its `min` no more than its `max`; the error names it as `name`
fn validate_span(name: &str, span: Span) -> Result<(), String> {
	let ordered = span.min.zip(span.max).is_none_or(|(min, max)| min <= max);
	check(ordered, &format!("{name}: min is more than max"))
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

	use super::{Animation, Filetypes, Mode, Modes, RoundedCornerOptions, Settings, Span, WindowOptions};

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

	/// Settings with only `window`'s section `name` given, as `pairs`
	fn window_section(name: &str, pairs: Vec<(&str, Value)>) -> Result<Settings, String> {
		parse(table(vec![("window", table(vec![(name, table(pairs))]))]))
	}

	#[test]
	fn nothing_given_keeps_the_defaults() {
		assert_eq!(parse(table(vec![])).unwrap(), Settings::default());
		assert_eq!(parse(table(vec![("window", table(vec![]))])).unwrap(), Settings::default());
		assert_eq!(window_section("scroll_animation", vec![]).unwrap(), Settings::default());
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
		let curve = |pairs| window_section("scroll_animation", vec![("curve", table(pairs))]);
		let kind = |name: &str| ("type", Value::from(name));
		let scrolled = |settings: Settings| settings.window.scroll_animation.curve;

		assert_eq!(scrolled(curve(vec![kind("none")]).unwrap()), Animation::None);
		assert_eq!(
			scrolled(curve(vec![kind("spring"), ("damping", Value::from(40))]).unwrap()),
			Animation::Spring { stiffness: 400.0, damping: 40.0 },
		);
		assert_eq!(scrolled(curve(vec![kind("ease_out")]).unwrap()), Animation::EaseOut { duration: 150.0 });
		let points = Value::Array([0.25, 0.1, 0.25, 1.0].into_iter().map(Value::from).collect());
		let bezier = curve(vec![kind("cubic_bezier"), ("points", points), ("duration", Value::from(200))]).unwrap();
		assert_eq!(scrolled(bezier), Animation::CubicBezier { points: [0.25, 0.1, 0.25, 1.0], duration: 200.0 });
	}

	#[test]
	fn mistakes_are_named() {
		let error = parse(table(vec![("windw", table(vec![]))])).unwrap_err();
		assert!(error.contains("unknown field `windw`") && error.contains("`window`"), "{error}");
		let error = window_section("scrol_animation", vec![]).unwrap_err();
		assert!(error.contains("unknown field `scrol_animation`") && error.contains("`scroll_animation`"), "{error}");

		let curve = |pairs| window_section("scroll_animation", vec![("curve", table(pairs))]);
		let error = curve(vec![("type", Value::from("sprng"))]).unwrap_err();
		assert!(error.contains("unknown variant `sprng`") && error.contains("`spring`"), "{error}");
		// Another curve's value
		let error = curve(vec![("type", Value::from("ease_out")), ("stiffness", Value::from(3))]).unwrap_err();
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
		let shadow = |pairs| window_section("shadow", pairs);
		let offset = Value::Array(vec![Value::from(2), Value::from(6)]);
		let settings = shadow(vec![("offset", offset), ("enabled", Value::from(false))])
			.unwrap()
			.window
			.shadow;
		assert_eq!((settings.offset, settings.enabled, settings.blur), ([2.0, 6.0], false, 8.0));

		let error = shadow(vec![("opacity", Value::from(1.5))]).unwrap_err();
		assert_eq!(error, "window.shadow.opacity must be from 0 to 1");
		let error = shadow(vec![("offset", Value::from(3))]).unwrap_err();
		assert!(error.contains("invalid type"), "{error}");
	}

	#[test]
	fn outline() {
		let outline = |pairs| window_section("outline", pairs);
		let settings = outline(vec![("enabled", Value::from(true)), ("highlight", Value::from("Comment"))])
			.unwrap()
			.window
			.outline;
		assert_eq!((settings.enabled, settings.width, settings.highlight.as_str()), (true, 1.0, "Comment"));
		assert!(!Settings::default().window.outline.enabled);
		let error = outline(vec![("width", Value::from(0))]).unwrap_err();
		assert_eq!(error, "window.outline.width must be more than 0");
	}

	#[test]
	fn rounded_corners() {
		let corners = |pairs| window_section("rounded_corners", pairs);
		let settings = corners(vec![("enabled", Value::from(true))]).unwrap().window.rounded_corners;
		assert_eq!((settings.enabled, settings.radius), (true, 8.0));
		assert!(!Settings::default().window.rounded_corners.enabled);
		let error = corners(vec![("radius", Value::from(-1))]).unwrap_err();
		assert_eq!(error, "window.rounded_corners.radius can't be negative");
	}

	#[test]
	fn padding() {
		let padding = |pairs| window_section("padding", pairs);
		let size = |pairs| padding(vec![("size", table(pairs))]).map(|settings| settings.window.padding.size);
		let settings = padding(vec![("enabled", Value::from(true)), ("size", Value::from(6))])
			.unwrap()
			.window
			.padding;
		assert_eq!((settings.enabled, settings.size.sides()), (true, [6.0; 4]));
		assert!(!Settings::default().window.padding.enabled);
		let error = padding(vec![("size", Value::from(-1))]).unwrap_err();
		assert_eq!(error, "window.padding.size can't be negative");

		// Each side its own, those left out none; an empty table, none at all
		let sides = size(vec![("top", Value::from(2)), ("left", Value::from(8.5))]).unwrap();
		assert_eq!(sides.sides(), [2.0, 0.0, 0.0, 8.5]);
		assert_eq!(size(vec![]).unwrap().sides(), [0.0; 4]);
		let error = size(vec![("lfet", Value::from(1))]).unwrap_err();
		assert!(error.contains("unknown field `lfet`"), "{error}");
		let error = size(vec![("right", Value::from(-2))]).unwrap_err();
		assert_eq!(error, "window.padding.size can't be negative");
	}

	#[test]
	fn settings_go_to_lua_as_setup_takes_them() {
		// As `require("sarthi").settings()` gets them: every section and key, by name, which setup() takes back
		let mut settings = Settings::default();
		let size = super::PaddingSize::Sides(super::PaddingSides { top: 2.0, ..Default::default() });
		settings.window.padding.size = size;
		settings.underline.ink_gaps = Some(1.5);
		settings.cursor_animation.overrides.push(super::CursorOverride {
			columns: super::Span { min: Some(3), max: None },
			modes: Some(super::Modes(vec![super::Mode::Insert, super::Mode::Cmdline])),
			trail: Some(0.5),
			..Default::default()
		});
		settings.window.scroll_animation.overrides =
			vec![super::ScrollOverride { lines: super::Span::exactly(1), curve: Some(Animation::None) }];
		let encoded = rmp_serde::to_vec_named(&settings).unwrap();
		assert_eq!(rmp_serde::from_slice::<Settings>(&encoded).unwrap(), settings);
		let encoded = rmp_serde::to_vec_named(&Settings::default()).unwrap();
		assert_eq!(rmp_serde::from_slice::<Settings>(&encoded).unwrap(), Settings::default());
	}

	#[test]
	fn window_options() {
		// `w:sarthi`: the sections of the settings' `window`, with the same keys, each replacing the setting's
		let shadow = table(vec![("opacity", Value::from(0.6)), ("offset", Value::Array(vec![1.into(), 2.into()]))]);
		let scroll = table(vec![("enabled", Value::from(false))]);
		let options: WindowOptions =
			rmpv::ext::from_value(table(vec![("shadow", shadow), ("scroll_animation", scroll)])).unwrap();
		let window = Settings::default().window.with(&options);
		assert_eq!((window.shadow.opacity, window.shadow.offset, window.shadow.blur), (0.6, [1.0, 2.0], 8.0));
		assert!(window.shadow.enabled && !window.scroll_animation.enabled);
		assert_eq!(rmpv::ext::from_value::<WindowOptions>(table(vec![])).unwrap(), WindowOptions::default());

		// Only the sections about a window, without their filetypes: those are only the settings'
		let filetypes = table(vec![("exclude_filetypes", Value::Array(vec![]))]);
		assert!(rmpv::ext::from_value::<WindowOptions>(table(vec![("shadow", filetypes)])).is_err());
		let cursor = table(vec![("cursor_animation", table(vec![]))]);
		assert!(rmpv::ext::from_value::<WindowOptions>(cursor).is_err());
		let opacity = table(vec![("shadow", table(vec![("opacity", Value::from(2))]))]);
		let options: WindowOptions = rmpv::ext::from_value(opacity).unwrap();
		let error = Settings::default().window.with(&options).validate().unwrap_err();
		assert_eq!(error, "shadow.opacity must be from 0 to 1");

		// Every value of a section, not only whether it's on
		let outline = table(vec![("width", Value::from(3)), ("highlight", Value::from("Comment"))]);
		let options: WindowOptions = rmpv::ext::from_value(table(vec![("outline", outline)])).unwrap();
		let outline = Settings::default().window.with(&options).outline;
		assert_eq!((outline.enabled, outline.width, outline.highlight.as_str()), (false, 3.0, "Comment"));
	}

	#[test]
	fn managed_windows_have_the_settings_frames() {
		let mut settings = Settings::default();
		let window = &mut settings.window;
		window.shadow.opacity = 0.5;
		window.outline.enabled = true;
		window.outline.width = 2.0;
		window.rounded_corners.enabled = true;
		window.padding.enabled = true;
		window.padding.exclude_filetypes = Filetypes::new(["help"]).unwrap();
		let radius = RoundedCornerOptions { radius: Some(3.0), ..Default::default() };
		let radius = WindowOptions { rounded_corners: radius, ..Default::default() };

		// Not managed: the settings' shadow, but only the window's own outline, corners and padding over the defaults
		let window = settings.window.with(&radius);
		assert_eq!(window.shadow.opacity, 0.5);
		assert_eq!((window.outline.enabled, window.outline.width), (false, 1.0));
		assert_eq!((window.rounded_corners.enabled, window.rounded_corners.radius), (false, 3.0));
		assert!(!window.padding.enabled);
		// The settings' filetypes, whichever
		assert!(window.padding.exclude_filetypes.matches("help"));

		// Managed: the settings', under the window's own
		let managed = WindowOptions { managed: Some(true), ..radius };
		let window = settings.window.with(&managed);
		assert_eq!((window.outline.enabled, window.outline.width), (true, 2.0));
		assert_eq!((window.rounded_corners.enabled, window.rounded_corners.radius), (true, 3.0));
		assert!(window.padding.enabled && window.padding.exclude_filetypes.matches("help"));

		let options: WindowOptions = rmpv::ext::from_value(table(vec![("managed", Value::from(true))])).unwrap();
		assert_eq!(options.managed, Some(true));
	}

	#[test]
	fn filetypes_are_glob_patterns() {
		let shadow = |filetypes: Vec<&str>| {
			let filetypes = Value::Array(filetypes.into_iter().map(Value::from).collect());
			window_section("shadow", vec![("exclude_filetypes", filetypes)])
		};
		let filetypes = shadow(vec!["help", "cmp_*", "dap-repl?"])
			.unwrap()
			.window
			.shadow
			.exclude_filetypes;
		assert!(filetypes.matches("help") && filetypes.matches("cmp_menu") && filetypes.matches("dap-repl2"));
		assert!(!filetypes.matches("helpx") && !filetypes.matches("lua") && !filetypes.matches("cmp"));
		assert!(!Filetypes::default().matches(""));
		assert_eq!(shadow(vec![]).unwrap().window.shadow.exclude_filetypes, Filetypes::default());

		let error = shadow(vec!["[help"]).unwrap_err();
		assert!(error.contains("`[help` isn't a glob pattern"), "{error}");

		// Back to Lua as given
		let encoded = rmp_serde::to_vec_named(&shadow(vec!["cmp_*"]).unwrap()).unwrap();
		assert_eq!(
			rmp_serde::from_slice::<Settings>(&encoded)
				.unwrap()
				.window
				.shadow
				.exclude_filetypes,
			Filetypes::new(["cmp_*"]).unwrap()
		);
	}

	#[test]
	fn background() {
		let background = |pairs| parse(table(vec![("background", table(pairs))]));
		let settings = background(vec![("opacity", Value::from(0.85)), ("blur", Value::from(true))])
			.unwrap()
			.background;
		assert_eq!((settings.opacity, settings.blur), (0.85, true));
		assert_eq!(Settings::default().background.opacity, 1.0);
		let error = background(vec![("opacity", Value::from(1.5))]).unwrap_err();
		assert_eq!(error, "background.opacity must be from 0 to 1");
	}

	#[test]
	fn cursor_animation() {
		let cursor = |pairs| parse(table(vec![("cursor_animation", table(pairs))]));
		let settings = cursor(vec![("trail", Value::from(0.2)), ("enabled", Value::from(false))])
			.unwrap()
			.cursor_animation;
		assert_eq!((settings.trail, settings.enabled), (0.2, false));
		assert!(Settings::default().cursor_animation.enabled);
		let error = cursor(vec![("trail", Value::from(2))]).unwrap_err();
		assert_eq!(error, "cursor_animation.trail must be from 0 to 1");
	}

	/// Settings with only `cursor_animation.overrides` given, as `rules`
	fn cursor_overrides(rules: Vec<Value>) -> Result<Settings, String> {
		parse(table(vec![("cursor_animation", table(vec![("overrides", Value::Array(rules))]))]))
	}

	#[test]
	fn cursor_overrides_decode() {
		let none = || ("curve", table(vec![("type", Value::from("none"))]));
		let rule = |pairs| cursor_overrides(vec![table(pairs)]).map(|settings| settings.cursor_animation.overrides);

		// A count is a number, or a table of `min` and `max`, either left out; an empty table is any count.
		let bounds = table(vec![("min", Value::from(3))]);
		let rules = rule(vec![("rows", Value::from(1)), ("columns", bounds), none()]).unwrap();
		assert_eq!((rules[0].rows, rules[0].columns), (Span::exactly(1), Span { min: Some(3), max: None }));
		assert_eq!(rules[0].curve, Some(Animation::None));
		assert!(rule(vec![("rows", table(vec![])), none()]).unwrap()[0].rows.is_any());

		// Modes as `vim.keymap.set()` takes them: one, or a list
		let modes = |value| rule(vec![("modes", value), none()]).map(|rules| rules[0].modes.clone());
		assert_eq!(modes(Value::from("i")).unwrap(), Some(Modes(vec![Mode::Insert])));
		let list = Value::Array(vec![Value::from("i"), Value::from("c")]);
		assert_eq!(modes(list).unwrap(), Some(Modes(vec![Mode::Insert, Mode::Cmdline])));
		let error = modes(Value::from("x")).unwrap_err();
		assert!(error.contains("unknown variant `x`") && error.contains("`i`"), "{error}");
		let error = modes(Value::Array(vec![Value::from("!")])).unwrap_err();
		assert!(error.contains("unknown variant `!`"), "{error}");

		// Mistakes, named by the rule's place in the list as Lua numbers it
		let error = rule(vec![("rows", table(vec![("min", Value::from(3)), ("max", Value::from(1))])), none()]);
		assert_eq!(error.unwrap_err(), "cursor_animation.overrides[1].rows: min is more than max");
		let error = rule(vec![("rows", Value::from(-1)), none()]).unwrap_err();
		assert!(error.contains("a number, or a table of min and max"), "{error}");
		let error = rule(vec![("rows", table(vec![("mni", Value::from(1))])), none()]).unwrap_err();
		assert!(error.contains("unknown field `mni`"), "{error}");
		let error = rule(vec![("rows", Value::from(1))]).unwrap_err();
		assert_eq!(error, "cursor_animation.overrides[1] changes nothing: give it a curve or a trail");
		let linear = table(vec![("type", Value::from("linear")), ("duration", Value::from(-1))]);
		let error = rule(vec![("curve", linear)]).unwrap_err();
		assert_eq!(error, "cursor_animation.overrides[1].curve.duration can't be negative");
		let error = cursor_overrides(vec![table(vec![none()]), table(vec![("trail", Value::from(2))])]).unwrap_err();
		assert_eq!(error, "cursor_animation.overrides[2].trail must be from 0 to 1");
	}

	#[test]
	fn the_values_of_a_move_are_those_of_the_overrides_that_apply() {
		let (spring, quick) =
			(Animation::Spring { stiffness: 700.0, damping: 53.0 }, Animation::EaseOut { duration: 40.0 });
		let mut cursor = Settings::default().cursor_animation;
		let step = |cursor: &super::CursorAnimationSettings, rows, columns, mode| {
			let step = cursor.for_move(&super::CursorMove { rows, columns, mode });
			(step.curve, step.trail)
		};

		// By default, short moves along a row are quick.
		assert_eq!(step(&cursor, 0, 2, None), (quick, 1.0));
		assert_eq!(step(&cursor, 0, 3, None), (spring, 1.0));
		assert_eq!(step(&cursor, 1, 0, None), (spring, 1.0));

		// Later ones win for what they give; what they leave out stays.
		cursor.overrides.push(super::CursorOverride {
			modes: Some(Modes(vec![Mode::Insert])),
			curve: Some(Animation::None),
			trail: Some(0.0),
			..Default::default()
		});
		cursor.overrides.push(super::CursorOverride {
			columns: Span::exactly(1),
			trail: Some(0.5),
			..Default::default()
		});
		assert_eq!(step(&cursor, 0, 1, Some(Mode::Insert)), (Animation::None, 0.5));
		assert_eq!(step(&cursor, 0, 2, Some(Mode::Insert)), (Animation::None, 0.0));
		assert_eq!(step(&cursor, 0, 1, Some(Mode::Normal)), (quick, 0.5));
		// A move into another mode is no mode's.
		assert_eq!(step(&cursor, 0, 5, None), (spring, 1.0));
	}

	#[test]
	fn scroll_overrides() {
		let overrides = |rules| window_section("scroll_animation", vec![("overrides", Value::Array(rules))]);
		let far = table(vec![
			("lines", table(vec![("min", Value::from(40))])),
			("curve", table(vec![("type", Value::from("none"))])),
		]);
		let scroll = overrides(vec![far]).unwrap().window.scroll_animation;
		assert_eq!((scroll.curve_for(39), scroll.curve_for(40)), (Animation::default(), Animation::None));

		let error = overrides(vec![table(vec![("lines", Value::from(1))])]).unwrap_err();
		assert_eq!(error, "window.scroll_animation.overrides[1] changes nothing: give it a curve");
	}
}
