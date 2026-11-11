//! How sarthi's settings (`rpc::sarthi::Settings`, set with `require("sarthi").setup()`) apply to the renderer

use std::time::Duration;

use editor::{FloatStyle, OutlineStyle};
use renderer::{CursorMotion, Easing, Motion, Shadow, UnderlineSize};
use rpc::sarthi::{Animation, FloatWindow, ScrolledWindow, Settings, WindowOptions, WindowVariable};

/// How the renderer moves scrolled text
pub fn scroll_motion(settings: &Settings) -> Motion {
	motion(settings.scroll.animation)
}

/// How the renderer moves the cursor
pub fn cursor_motion(settings: &Settings) -> CursorMotion {
	let cursor = &settings.cursor;
	CursorMotion { motion: motion(cursor.animation), short: motion(cursor.short_animation), trail: cursor.trail }
}

/// `animation` as the renderer takes it
fn motion(animation: Animation) -> Motion {
	let ease = |easing, duration: f32| Motion::Ease { easing, duration: Duration::from_secs_f32(duration / 1000.0) };
	match animation {
		// Over no time at all, the value is in place at once.
		Animation::None => ease(Easing::Linear, 0.0),
		Animation::Spring { stiffness, damping } => Motion::Spring { stiffness, damping },
		Animation::Linear { duration } => ease(Easing::Linear, duration),
		Animation::EaseOut { duration } => ease(Easing::EaseOut, duration),
		Animation::EaseInOut { duration } => ease(Easing::EaseInOut, duration),
		Animation::CubicBezier { points: [x1, y1, x2, y2], duration } => {
			ease(Easing::CubicBezier(x1, y1, x2, y2), duration)
		},
	}
}

/// What a window's `w:sarthi` gives it, if it's one sarthi can decode with values that make sense; otherwise what's
/// wrong with it
pub fn window_options(settings: &Settings, variable: &WindowVariable) -> Result<WindowOptions, String> {
	let WindowVariable::Options(options) = variable else {
		return Err(String::from("has a key sarthi doesn't know, or a value of the wrong type"));
	};
	if *options != WindowOptions::default() {
		settings.for_window(options).validate()?;
	}
	Ok(options.clone())
}

/// How `window`'s text moves when it scrolls, with `options` from its `w:sarthi`: slides as its scroll settings say,
/// or jumps to its new place for `None`. It slides if `options` say so, or else if sliding is enabled and its filetype
/// isn't excluded.
pub fn window_scroll(settings: &Settings, window: &ScrolledWindow, options: &WindowOptions) -> Option<Motion> {
	let excluded = settings.scroll.exclude_filetypes.contains(&window.filetype);
	let scroll = settings.scroll.with(&options.scroll);
	let enabled = options.scroll.enabled.unwrap_or(scroll.enabled && !excluded);
	enabled.then(|| motion(scroll.animation))
}

/// How floating or external window `window` looks besides its cells, with `options` from its `w:sarthi`, in logical
/// pixels (see [`style`])
pub fn float_style(settings: &Settings, window: &FloatWindow, options: &WindowOptions) -> FloatStyle {
	let mut style = style(settings, options, &window.filetype, window.winblend);
	// In the color Neovim has for its highlight group, which sarthi may not know
	if let Some(outline) = &mut style.outline {
		outline.color = window.outline_color;
	}
	FloatStyle { parent: window.parent, ..style }
}

/// How floating and external windows look until sarthi learns about them
pub fn default_float_style(settings: &Settings) -> FloatStyle {
	style(settings, &WindowOptions::default(), "", 0)
}

/// How a floating or external window showing `filetype`, `winblend` transparent, looks besides its cells: the settings,
/// with what `options` gives in their place. Each part shows if `options` say so, or else if it's enabled and
/// `filetype` isn't excluded; none around a fully transparent window.
fn style(settings: &Settings, options: &WindowOptions, filetype: &str, winblend: u32) -> FloatStyle {
	let shows = |enabled: Option<bool>, default: bool, excluded: &[String]| {
		winblend < 100 && enabled.unwrap_or(default && !excluded.iter().any(|excluded| excluded == filetype))
	};
	let shadow = settings.shadow.with(&options.shadow);
	let shadow = shows(options.shadow.enabled, shadow.enabled, &shadow.exclude_filetypes).then_some(Shadow {
		blur: shadow.blur,
		offset: shadow.offset,
		color: 0x000000,
		opacity: shadow.opacity,
	});
	let outline = settings.outline.with(&options.outline);
	let outline = shows(options.outline.enabled, outline.enabled, &outline.exclude_filetypes).then_some(OutlineStyle {
		width: outline.width,
		highlight: outline.highlight,
		color: None,
	});
	let corners = settings.corners.with(&options.corners);
	let corners = shows(options.corners.enabled, corners.enabled, &corners.exclude_filetypes).then_some(corners.radius);
	FloatStyle { shadow, outline, corners, parent: None }
}

/// How the renderer draws underlines, in logical pixels
pub fn underlines(settings: &Settings) -> UnderlineSize {
	let underline = &settings.underline;
	UnderlineSize {
		offset: underline.offset,
		thickness: underline.thickness,
		wave_height: underline.wave_height,
		ink_gaps: underline.ink_gaps,
	}
}

#[cfg(test)]
mod tests {
	use renderer::Motion;
	use rpc::neovim::Window;
	use rpc::sarthi::{
		Animation, CornerOptions, FloatWindow, OutlineOptions, ScrollOptions, ScrolledWindow, Settings, ShadowOptions,
		WindowOptions, WindowVariable,
	};
	use serde::de::IgnoredAny;

	fn float(filetype: &str) -> FloatWindow {
		FloatWindow {
			win: Window(1000),
			filetype: filetype.into(),
			sarthi: WindowVariable::default(),
			winblend: 0,
			outline_color: None,
			parent: None,
		}
	}

	fn shadow(options: ShadowOptions) -> WindowOptions {
		WindowOptions { shadow: options, ..Default::default() }
	}

	#[test]
	fn shadows() {
		let mut settings = Settings::default();
		let none = WindowOptions::default();
		let style = super::float_style(&settings, &float("lua"), &none);
		let cast = style.shadow.unwrap();
		assert_eq!((cast.blur, cast.offset, cast.opacity), (16.0, [0.0, 4.0], 0.3));
		assert_eq!(super::default_float_style(&settings).shadow, Some(cast));

		// The window's own values, each in place of the setting's
		let own = shadow(ShadowOptions { opacity: Some(0.6), offset: Some([1.0, 2.0]), ..Default::default() });
		let cast = super::float_style(&settings, &float("lua"), &own).shadow.unwrap();
		assert_eq!((cast.blur, cast.offset, cast.opacity), (16.0, [1.0, 2.0], 0.6));

		settings.shadow.exclude_filetypes = vec![String::from("satellite")];
		assert!(super::float_style(&settings, &float("satellite"), &none).shadow.is_none());
		// The window's `enabled` wins, over excluded filetypes and the setting.
		let on = shadow(ShadowOptions { enabled: Some(true), ..Default::default() });
		let off = shadow(ShadowOptions { enabled: Some(false), ..Default::default() });
		assert!(super::float_style(&settings, &float("satellite"), &on).shadow.is_some());
		assert!(super::float_style(&settings, &float("lua"), &off).shadow.is_none());
		settings.shadow.enabled = false;
		assert!(super::float_style(&settings, &float("lua"), &none).shadow.is_none());
		assert!(super::float_style(&settings, &float("lua"), &on).shadow.is_some());
		// Fully transparent: no shadow, whatever the window says
		let transparent = FloatWindow { winblend: 100, ..float("lua") };
		assert!(super::float_style(&settings, &transparent, &on).shadow.is_none());
	}

	#[test]
	fn outlines() {
		let mut settings = Settings::default();
		let outline = |options: OutlineOptions| WindowOptions { outline: options, ..Default::default() };
		let none = WindowOptions::default();
		// Off by default, unless the window says so, with the settings' values where it gives none
		assert_eq!(super::float_style(&settings, &float("lua"), &none).outline, None);
		let on = outline(OutlineOptions { enabled: Some(true), width: Some(2.5), ..Default::default() });
		let style = super::float_style(&settings, &float("lua"), &on).outline.unwrap();
		assert_eq!((style.width, style.highlight.as_str()), (2.5, "FloatBorder"));

		settings.outline.enabled = true;
		settings.outline.exclude_filetypes = vec![String::from("satellite")];
		let style = super::float_style(&settings, &float("lua"), &none).outline.unwrap();
		assert_eq!(style.width, 1.0);
		assert_eq!(super::default_float_style(&settings).outline, Some(style));
		assert_eq!(super::float_style(&settings, &float("satellite"), &none).outline, None);
		let off = outline(OutlineOptions { enabled: Some(false), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &off).outline, None);
		// Its own color, still on as the settings say
		let comment = outline(OutlineOptions { highlight: Some(String::from("Comment")), ..Default::default() });
		let style = super::float_style(&settings, &float("lua"), &comment).outline.unwrap();
		assert_eq!((style.width, style.highlight.as_str()), (1.0, "Comment"));
		let transparent = FloatWindow { winblend: 100, ..float("lua") };
		assert_eq!(super::float_style(&settings, &transparent, &on).outline, None);

		// In the color Neovim has for the group, if it has one
		let colored = FloatWindow { outline_color: Some(0xFF0000), ..float("lua") };
		assert_eq!(super::float_style(&settings, &colored, &none).outline.unwrap().color, Some(0xFF0000));
	}

	#[test]
	fn corners() {
		let mut settings = Settings::default();
		let corners = |options: CornerOptions| WindowOptions { corners: options, ..Default::default() };
		let none = WindowOptions::default();
		// Square by default, unless the window says so, at the settings' radius or its own
		assert_eq!(super::float_style(&settings, &float("lua"), &none).corners, None);
		let on = corners(CornerOptions { enabled: Some(true), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &on).corners, Some(8.0));

		settings.corners.enabled = true;
		settings.corners.exclude_filetypes = vec![String::from("satellite")];
		let radius = corners(CornerOptions { radius: Some(4.0), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &radius).corners, Some(4.0));
		assert_eq!(super::float_style(&settings, &float("satellite"), &radius).corners, None);
		let off = corners(CornerOptions { enabled: Some(false), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &off).corners, None);
	}

	#[test]
	fn mistakes_in_window_options() {
		let settings = Settings::default();
		let opacity = shadow(ShadowOptions { opacity: Some(2.0), ..Default::default() });
		let mistake = super::window_options(&settings, &WindowVariable::Options(opacity)).unwrap_err();
		assert_eq!(mistake, "shadow.opacity must be from 0 to 1");
		assert!(super::window_options(&settings, &WindowVariable::Invalid(IgnoredAny)).is_err());
		assert_eq!(super::window_options(&settings, &WindowVariable::default()), Ok(WindowOptions::default()));
	}

	#[test]
	fn which_windows_slide() {
		let mut settings = Settings::default();
		settings.scroll.exclude_filetypes = vec![String::from("cmp_menu")];
		let window = |filetype: &str| ScrolledWindow {
			win: Window(1000),
			filetype: filetype.into(),
			sarthi: WindowVariable::default(),
		};
		let scroll = |options: ScrollOptions| WindowOptions { scroll: options, ..Default::default() };
		let (none, on, off) = (
			WindowOptions::default(),
			scroll(ScrollOptions { enabled: Some(true), ..Default::default() }),
			scroll(ScrollOptions { enabled: Some(false), ..Default::default() }),
		);
		let slides =
			|settings: &Settings, filetype, options| super::window_scroll(settings, &window(filetype), options);

		assert!(slides(&settings, "lua", &none).is_some());
		assert!(slides(&settings, "cmp_menu", &none).is_none());
		// The window's `enabled` wins.
		assert!(slides(&settings, "cmp_menu", &on).is_some());
		assert!(slides(&settings, "lua", &off).is_none());
		// Nothing is excluded by default, and nothing slides with sliding off, unless the window says so.
		assert!(slides(&Settings::default(), "cmp_menu", &none).is_some());
		settings.scroll.enabled = false;
		assert!(slides(&settings, "lua", &none).is_none());
		assert!(slides(&settings, "lua", &on).is_some());

		// Its own animation
		let linear = scroll(ScrollOptions { animation: Some(Animation::None), ..Default::default() });
		let motion = slides(&Settings::default(), "lua", &linear);
		assert!(matches!(motion, Some(Motion::Ease { duration, .. }) if duration.is_zero()), "{motion:?}");
	}
}
