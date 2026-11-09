use std::collections::HashMap;

use nvim::{DefaultColorsSetEvent, HlAttrDefineEvent, HlAttrs, HlGroupSetEvent};
use renderer::Underline;

/// Default colors until Neovim sends its own, matching what Neovim uses when none are set
const INITIAL_DEFAULT_COLORS: DefaultColors =
	DefaultColors { foreground: 0xFFFFFF, background: 0x000000, special: 0xFF0000 };

/// Neovim's default colors (`default_colors_set`), as `0xRRGGBB`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultColors {
	pub foreground: u32,
	pub background: u32,
	pub special: u32,
}

/// How a cell looks, with every color resolved to `0xRRGGBB`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
	pub foreground: u32,
	pub background: u32,

	/// Color of underlines and undercurls
	pub special: u32,

	pub bold: bool,
	pub italic: bool,

	pub underline: Option<Underline>,
	pub strikethrough: bool,
	pub overline: bool,
}

/// Neovim's highlight table, from `hl_attr_define`, `default_colors_set` and `hl_group_set`
/// (`:help ui-event-hl_attr_define`)
#[derive(Debug)]
pub struct HighlightState {
	/// RGB attributes by highlight id. Colors a highlight doesn't set stay `None`, so they follow the
	/// default colors even when those change later.
	attrs: HashMap<u32, HlAttrs>,
	defaults: DefaultColors,
	groups: HashMap<String, u32>,
}

impl Default for HighlightState {
	fn default() -> Self {
		Self::new()
	}
}

impl HighlightState {
	pub fn new() -> Self {
		Self { attrs: HashMap::new(), defaults: INITIAL_DEFAULT_COLORS, groups: HashMap::new() }
	}

	pub fn defaults(&self) -> DefaultColors {
		self.defaults
	}

	/// Adds or replaces highlights. Neovim may reuse ids, and then redraws the affected cells itself.
	pub fn set_attrs(&mut self, attrs: Vec<HlAttrDefineEvent>) {
		self.attrs.extend(attrs.into_iter().map(|attr| (attr.id, attr.rgb_attr)));
	}

	/// Applies the events in order, so the last one wins.
	pub fn set_default_colors(&mut self, colors: Vec<DefaultColorsSetEvent>) {
		if let Some(colors) = colors.last() {
			self.defaults =
				DefaultColors { foreground: colors.rgb_fg, background: colors.rgb_bg, special: colors.rgb_sp };
		}
	}

	pub fn set_hl_groups(&mut self, groups: Vec<HlGroupSetEvent>) {
		self.groups.extend(groups.into_iter().map(|group| (group.name, group.hl_id)));
	}

	/// The highlight id of Neovim's UI highlight group `name` (`:help highlight-groups`), or 0, the default highlight,
	/// until Neovim sends it with `hl_group_set`
	pub fn group(&self, name: &str) -> u32 {
		self.groups.get(name).copied().unwrap_or(0)
	}

	/// The style of cells with highlight `hl_id`. Id 0 is the default highlight: the default colors and no
	/// styles. An id that hasn't been defined is treated the same way.
	pub fn resolve(&self, hl_id: u32) -> CellStyle {
		let defaults = self.defaults;
		let attrs = (hl_id != 0).then(|| self.attrs.get(&hl_id)).flatten();

		let Some(attrs) = attrs else {
			return CellStyle {
				foreground: defaults.foreground,
				background: defaults.background,
				special: defaults.special,
				bold: false,
				italic: false,
				underline: None,
				strikethrough: false,
				overline: false,
			};
		};

		let mut foreground = attrs.foreground.unwrap_or(defaults.foreground);
		let mut background = attrs.background.unwrap_or(defaults.background);
		if attrs.reverse {
			std::mem::swap(&mut foreground, &mut background);
		}

		CellStyle {
			foreground,
			background,
			special: attrs.special.unwrap_or(defaults.special),
			bold: attrs.bold,
			italic: attrs.italic,
			underline: underline(attrs),
			strikethrough: attrs.strikethrough,
			overline: attrs.overline,
		}
	}
}

/// The underline style of `attrs`. Neovim sends one, but if several are set, the more specific one wins.
fn underline(attrs: &HlAttrs) -> Option<Underline> {
	[
		(attrs.undercurl, Underline::Curly),
		(attrs.underdouble, Underline::Double),
		(attrs.underdotted, Underline::Dotted),
		(attrs.underdashed, Underline::Dashed),
		(attrs.underline, Underline::Single),
	]
	.into_iter()
	.find_map(|(set, style)| set.then_some(style))
}

#[cfg(test)]
pub(crate) mod tests {
	use nvim::{DefaultColorsSetEvent, HlAttrDefineEvent, HlAttrs};

	use super::{CellStyle, DefaultColors, HighlightState, INITIAL_DEFAULT_COLORS};
	use renderer::Underline;

	const FG: u32 = 0xC7C7C7;
	const BG: u32 = 0x1C1C1C;
	const SP: u32 = 0xFF0000;

	fn colors(foreground: u32, background: u32, special: u32) -> DefaultColorsSetEvent {
		DefaultColorsSetEvent { rgb_fg: foreground, rgb_bg: background, rgb_sp: special, cterm_fg: 0, cterm_bg: 0 }
	}

	fn define(id: u32, rgb_attr: HlAttrs) -> HlAttrDefineEvent {
		HlAttrDefineEvent { id, rgb_attr: rgb_attr.clone(), cterm_attr: rgb_attr, info: vec![] }
	}

	/// Attributes that set nothing
	pub(crate) fn attrs() -> HlAttrs {
		HlAttrs {
			foreground: None,
			background: None,
			special: None,
			reverse: false,
			italic: false,
			bold: false,
			strikethrough: false,
			underline: false,
			undercurl: false,
			underdouble: false,
			underdotted: false,
			underdashed: false,
			altfont: false,
			dim: false,
			blink: false,
			conceal: false,
			overline: false,
			blend: None,
			url: None,
		}
	}

	fn state() -> HighlightState {
		let mut state = HighlightState::new();
		state.set_default_colors(vec![colors(FG, BG, SP)]);
		state
	}

	fn plain(foreground: u32, background: u32) -> CellStyle {
		CellStyle {
			foreground,
			background,
			special: SP,
			bold: false,
			italic: false,
			underline: None,
			strikethrough: false,
			overline: false,
		}
	}

	#[test]
	fn initial_defaults() {
		let state = HighlightState::new();
		assert_eq!(state.defaults(), INITIAL_DEFAULT_COLORS);
		assert_eq!(state.resolve(0).background, 0x000000);
	}

	#[test]
	fn last_default_colors_win() {
		let mut state = HighlightState::new();
		state.set_default_colors(vec![colors(1, 2, 3), colors(FG, BG, SP)]);
		assert_eq!(state.defaults(), DefaultColors { foreground: FG, background: BG, special: SP });
	}

	#[test]
	fn default_highlight() {
		assert_eq!(state().resolve(0), plain(FG, BG));
	}

	#[test]
	fn id_zero_ignores_a_definition() {
		let mut state = state();
		state.set_attrs(vec![define(0, HlAttrs { foreground: Some(0x123456), bold: true, ..attrs() })]);
		assert_eq!(state.resolve(0), plain(FG, BG));
	}

	#[test]
	fn undefined_id() {
		assert_eq!(state().resolve(42), plain(FG, BG));
	}

	#[test]
	fn own_colors_and_styles() {
		let mut state = state();
		let highlight = HlAttrs {
			foreground: Some(0x112233),
			background: Some(0x445566),
			special: Some(0x778899),
			bold: true,
			italic: true,
			..attrs()
		};
		state.set_attrs(vec![define(5, highlight)]);
		let expected = CellStyle { special: 0x778899, bold: true, italic: true, ..plain(0x112233, 0x445566) };
		assert_eq!(state.resolve(5), expected);
	}

	#[test]
	fn missing_colors_use_defaults() {
		let mut state = state();
		state.set_attrs(vec![define(5, HlAttrs { foreground: Some(0x112233), ..attrs() })]);
		assert_eq!(state.resolve(5), plain(0x112233, BG));
	}

	#[test]
	fn reverse_swaps_resolved_colors() {
		let mut state = state();
		state.set_attrs(vec![
			// Like StatusLine with `reverse` and no colors of its own
			define(1, HlAttrs { reverse: true, ..attrs() }),
			define(2, HlAttrs { foreground: Some(0x112233), reverse: true, ..attrs() }),
		]);
		assert_eq!(state.resolve(1), plain(BG, FG));
		assert_eq!(state.resolve(2), plain(BG, 0x112233));
	}

	#[test]
	fn missing_colors_follow_changed_defaults() {
		let mut state = state();
		state.set_attrs(vec![define(5, HlAttrs { foreground: Some(0x112233), ..attrs() })]);
		state.set_default_colors(vec![colors(0xEEEEEE, 0x222222, 0x00FF00)]);
		let expected = CellStyle { special: 0x00FF00, ..plain(0x112233, 0x222222) };
		assert_eq!(state.resolve(5), expected);
	}

	#[test]
	fn redefined_id_replaces() {
		let mut state = state();
		state.set_attrs(vec![define(5, HlAttrs { bold: true, ..attrs() })]);
		state.set_attrs(vec![define(5, HlAttrs { italic: true, ..attrs() })]);
		assert_eq!(state.resolve(5), CellStyle { italic: true, ..plain(FG, BG) });
	}

	#[test]
	fn uses_rgb_attributes() {
		let mut state = state();
		let event = HlAttrDefineEvent {
			id: 5,
			rgb_attr: HlAttrs { foreground: Some(0x112233), ..attrs() },
			cterm_attr: HlAttrs { foreground: Some(9), bold: true, ..attrs() },
			info: vec![],
		};
		state.set_attrs(vec![event]);
		assert_eq!(state.resolve(5), plain(0x112233, BG));
	}

	#[test]
	fn decorations() {
		let mut state = state();
		state.set_attrs(vec![
			define(1, HlAttrs { underline: true, special: Some(0x00FF00), ..attrs() }),
			define(2, HlAttrs { undercurl: true, strikethrough: true, ..attrs() }),
			define(3, HlAttrs { underdouble: true, overline: true, ..attrs() }),
			define(4, HlAttrs { underdotted: true, ..attrs() }),
			define(5, HlAttrs { underdashed: true, ..attrs() }),
			// Several underline styles: the more specific one wins
			define(6, HlAttrs { underline: true, undercurl: true, ..attrs() }),
		]);

		assert_eq!(
			state.resolve(1),
			CellStyle { underline: Some(Underline::Single), special: 0x00FF00, ..plain(FG, BG) }
		);
		assert_eq!(
			state.resolve(2),
			CellStyle { underline: Some(Underline::Curly), strikethrough: true, ..plain(FG, BG) }
		);
		assert_eq!(state.resolve(3), CellStyle { underline: Some(Underline::Double), overline: true, ..plain(FG, BG) });
		assert_eq!(state.resolve(4).underline, Some(Underline::Dotted));
		assert_eq!(state.resolve(5).underline, Some(Underline::Dashed));
		assert_eq!(state.resolve(6).underline, Some(Underline::Curly));
	}
}
