//! How sarthi's settings (`rpc::sarthi::Settings`, set with `require("sarthi").setup()`) apply to the renderer

use std::time::Duration;

use editor::{FloatStyle, OutlineStyle};
use renderer::{CursorMotion, Easing, Motion, Shadow, UnderlineSize};
use rpc::sarthi::{Animation, Filetypes, FloatWindow, ScrolledWindow, Settings, WindowOptions, WindowVariable};

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
/// or jumps to its new place for `None`. It slides if its `w:sarthi`, or else the settings, enable it, and its filetype
/// isn't excluded.
pub fn window_scroll(settings: &Settings, window: &ScrolledWindow, options: &WindowOptions) -> Option<Motion> {
	let scroll = settings.scroll.with(&options.scroll);
	let enabled = scroll.enabled && !scroll.exclude_filetypes.matches(&window.filetype);
	enabled.then(|| motion(scroll.animation))
}

/// How floating or external window `window` looks besides its cells, with `options` from its `w:sarthi`, in logical
/// pixels (see [`style`])
pub fn float_style(settings: &Settings, window: &FloatWindow, options: &WindowOptions) -> FloatStyle {
	let mut style = style(&settings.for_window(options), &window.filetype, window.winblend);
	// In the color Neovim has for its highlight group, which sarthi may not know
	if let Some(outline) = &mut style.outline {
		outline.color = window.outline_color;
	}
	FloatStyle { parent: window.parent, ..style }
}

/// How floating and external windows look until sarthi learns about them: as one with no `w:sarthi`
pub fn default_float_style(settings: &Settings) -> FloatStyle {
	style(&settings.for_window(&WindowOptions::default()), "", 0)
}

/// How a floating or external window showing `filetype`, `winblend` transparent, looks besides its cells, with
/// `settings` for it (see [`Settings::for_window`]). Each part shows if it's enabled and `filetype` isn't excluded; none
/// around a fully transparent window.
fn style(settings: &Settings, filetype: &str, winblend: u32) -> FloatStyle {
	let shows = |enabled: bool, excluded: &Filetypes| winblend < 100 && enabled && !excluded.matches(filetype);
	let shadow = &settings.shadow;
	let shadow = shows(shadow.enabled, &shadow.exclude_filetypes).then_some(Shadow {
		blur: shadow.blur,
		offset: shadow.offset,
		color: 0x000000,
		opacity: shadow.opacity,
	});
	let outline = &settings.outline;
	let outline = shows(outline.enabled, &outline.exclude_filetypes).then(|| OutlineStyle {
		width: outline.width,
		highlight: outline.highlight.clone(),
		color: None,
	});
	let corners = &settings.corners;
	let corners = shows(corners.enabled, &corners.exclude_filetypes).then_some(corners.radius);
	let padding = &settings.padding;
	let padding = shows(padding.enabled, &padding.exclude_filetypes).then_some(padding.size.sides());
	FloatStyle { shadow, outline, corners, padding, parent: None }
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
		Animation, CornerOptions, Filetypes, FloatWindow, OutlineOptions, PaddingOptions, PaddingSides, PaddingSize,
		ScrollOptions, ScrolledWindow, Settings, ShadowOptions, WindowOptions, WindowVariable,
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
		// Every float casts the settings' shadow.
		let style = super::float_style(&settings, &float("lua"), &none);
		let cast = style.shadow.unwrap();
		assert_eq!((cast.blur, cast.offset, cast.opacity), (16.0, [0.0, 4.0], 0.3));
		assert_eq!(super::default_float_style(&settings).shadow, Some(cast));

		// The window's own values, each in place of the setting's
		let own = shadow(ShadowOptions { opacity: Some(0.6), offset: Some([1.0, 2.0]), ..Default::default() });
		let cast = super::float_style(&settings, &float("lua"), &own).shadow.unwrap();
		assert_eq!((cast.blur, cast.offset, cast.opacity), (16.0, [1.0, 2.0], 0.6));

		// Excluded filetypes win, over the window's `enabled` too.
		settings.shadow.exclude_filetypes = Filetypes::new(["satellite", "cmp_*"]).unwrap();
		let on = shadow(ShadowOptions { enabled: Some(true), ..Default::default() });
		let off = shadow(ShadowOptions { enabled: Some(false), ..Default::default() });
		assert!(super::float_style(&settings, &float("satellite"), &none).shadow.is_none());
		assert!(super::float_style(&settings, &float("cmp_menu"), &on).shadow.is_none());
		assert!(super::float_style(&settings, &float("lua"), &off).shadow.is_none());
		// The window's `enabled` wins over the setting's.
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
		settings.outline.enabled = true;
		settings.outline.width = 2.0;
		settings.outline.exclude_filetypes = Filetypes::new(["satellite"]).unwrap();
		let outline = |options: OutlineOptions| WindowOptions { outline: options, ..Default::default() };
		let none = WindowOptions::default();
		let managed = WindowOptions { managed: Some(true), ..Default::default() };

		// Only a managed window has the settings' outline; others none, as before sarthi learns about them.
		assert_eq!(super::float_style(&settings, &float("lua"), &none).outline, None);
		assert_eq!(super::default_float_style(&settings).outline, None);
		let style = super::float_style(&settings, &float("lua"), &managed).outline.unwrap();
		assert_eq!((style.width, style.highlight.as_str()), (2.0, "FloatBorder"));
		// Or one a window turns on itself, with the defaults where it gives no values
		let on = outline(OutlineOptions { enabled: Some(true), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &on).outline.unwrap().width, 1.0);

		// Its own values over the settings'
		let comment = WindowOptions {
			outline: OutlineOptions { highlight: Some(String::from("Comment")), ..Default::default() },
			..managed.clone()
		};
		let style = super::float_style(&settings, &float("lua"), &comment).outline.unwrap();
		assert_eq!((style.width, style.highlight.as_str()), (2.0, "Comment"));
		let off = WindowOptions { outline: OutlineOptions { enabled: Some(false), ..Default::default() }, ..managed };
		assert_eq!(super::float_style(&settings, &float("lua"), &off).outline, None);
		// Excluded filetypes win.
		assert_eq!(super::float_style(&settings, &float("satellite"), &on).outline, None);
		let transparent = FloatWindow { winblend: 100, ..float("lua") };
		assert_eq!(super::float_style(&settings, &transparent, &on).outline, None);

		// In the color Neovim has for the group, if it has one
		let colored = FloatWindow { outline_color: Some(0xFF0000), ..float("lua") };
		assert_eq!(super::float_style(&settings, &colored, &on).outline.unwrap().color, Some(0xFF0000));
	}

	#[test]
	fn corners() {
		let mut settings = Settings::default();
		settings.corners.enabled = true;
		settings.corners.radius = 6.0;
		settings.corners.exclude_filetypes = Filetypes::new(["satellite"]).unwrap();
		let corners =
			|managed, options: CornerOptions| WindowOptions { managed, corners: options, ..Default::default() };
		let none = WindowOptions::default();

		// Square, unless managed or the window turns them on, at the settings' radius or the default
		assert_eq!(super::float_style(&settings, &float("lua"), &none).corners, None);
		assert_eq!(
			super::float_style(&settings, &float("lua"), &corners(Some(true), Default::default())).corners,
			Some(6.0)
		);
		let on = CornerOptions { enabled: Some(true), ..Default::default() };
		assert_eq!(super::float_style(&settings, &float("lua"), &corners(None, on.clone())).corners, Some(8.0));

		let radius = corners(Some(true), CornerOptions { radius: Some(4.0), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &radius).corners, Some(4.0));
		assert_eq!(super::float_style(&settings, &float("satellite"), &corners(None, on)).corners, None);
		let off = corners(Some(true), CornerOptions { enabled: Some(false), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &off).corners, None);
	}

	#[test]
	fn padding() {
		let mut settings = Settings::default();
		settings.padding.enabled = true;
		settings.padding.size = PaddingSize::All(6.0);
		settings.padding.exclude_filetypes = Filetypes::new(["satellite"]).unwrap();
		let padding =
			|managed, options: PaddingOptions| WindowOptions { managed, padding: options, ..Default::default() };
		let none = WindowOptions::default();

		// None, unless managed or the window turns it on, at the settings' size or the default
		assert_eq!(super::float_style(&settings, &float("lua"), &none).padding, None);
		let managed = padding(Some(true), Default::default());
		assert_eq!(super::float_style(&settings, &float("lua"), &managed).padding, Some([6.0; 4]));
		let on = PaddingOptions { enabled: Some(true), ..Default::default() };
		assert_eq!(super::float_style(&settings, &float("lua"), &padding(None, on.clone())).padding, Some([4.0; 4]));

		// Each side its own
		let sides = PaddingSize::Sides(PaddingSides { top: 1.0, left: 3.0, ..Default::default() });
		let own = padding(Some(true), PaddingOptions { size: Some(sides), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &own).padding, Some([1.0, 0.0, 0.0, 3.0]));
		assert_eq!(super::float_style(&settings, &float("satellite"), &padding(None, on)).padding, None);
		let off = padding(Some(true), PaddingOptions { enabled: Some(false), ..Default::default() });
		assert_eq!(super::float_style(&settings, &float("lua"), &off).padding, None);
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
		settings.scroll.exclude_filetypes = Filetypes::new(["cmp_*"]).unwrap();
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
		// Excluded filetypes win, over the window's `enabled` too.
		assert!(slides(&settings, "cmp_menu", &on).is_none());
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
