//! `require('sarthi').api`: Neovim's window functions that take a window config, taking a `sarthi` table in it too.
//!
//! The `sarthi` table gives a window its own values of the settings' sections about windows, kept in its `w:sarthi`
//! (marked `managed`, so the settings' outline, corners and padding apply to it too), and for external windows its
//! place and fonts, which Sarthi keeps itself. Neovim's own config goes to Neovim as given.
//!
//! The tables are Lua's, as Neovim sends them, so they're handled as msgpack values: a window's `w:sarthi` is merged
//! key by key with what's given, as plugins set it too.

use rmpv::Value;
use rpc::neovim::{Buffer, Window};
use rpc::sarthi::{PlaceRelative, WindowFont, WindowOptions, WindowPlace};

use super::{Ctx, neovim_error};
use crate::place;

/// A Lua table, as msgpack carries it: its keys and values
type Table = Vec<(Value, Value)>;

/// The sections of the settings a window can have its own values of, kept in `w:sarthi`
const OPTIONS: [&str; 5] = ["shadow", "outline", "rounded_corners", "padding", "scroll_animation"];

/// What only external windows take, and why
const EXTERNAL_ONLY: [(&str, &str); 5] = [
	("place", "floating windows are placed by Neovim"),
	("font", "floating windows share the main grid's cells"),
	("outline", "Neovim places floating windows without room for it"),
	("rounded_corners", "they'd cut a floating window's text"),
	("padding", "Neovim places floating windows without room for it"),
];

/// `sarthi.place` and `sarthi.font` as a window was last given them, as Lua gave them
#[derive(Debug, Clone, Default)]
pub(super) struct Given {
	place: Option<Value>,
	font: Option<Value>,
}

/// Opens a window like `nvim_open_win()`, with a `sarthi` table in `config`. Managed even without one: it has the
/// settings' outline, corners and padding. A mistake in the `sarthi` table closes the window again, so the call does
/// nothing but fail.
pub(super) fn open(ctx: &Ctx, buf: Buffer, enter: bool, config: Value) -> Result<Window, String> {
	let (neovim, sarthi) = split(config)?;
	let external = get(&neovim, "external").is_some_and(truthy);
	let sarthi = sarthi.unwrap_or_else(|| Value::Map(Vec::new()));
	let options = check(ctx, Some(&sarthi), Vec::new(), external)?;
	let given = Given { place: field(&sarthi, "place"), font: field(&sarthi, "font") };
	// What it's placed relative to, before entering it makes it the current window
	let place = resolve(ctx, given.place.as_ref())?;

	let win = ctx.nvim.open_win(buf, enter, Value::Map(neovim)).map_err(neovim_error)?;
	if let Err(mistake) = apply(ctx, win, given, place, options) {
		ctx.nvim.win_close(win, true).ok();
		return Err(mistake);
	}
	Ok(win)
}

/// Changes a window's config like `nvim_win_set_config()`, with a `sarthi` table in `config`: what it gives replaces
/// what was given before, key by key, and a section's values key by key too.
pub(super) fn set_config(ctx: &Ctx, win: Window, config: Value) -> Result<(), String> {
	let win = or_current(ctx, win)?;
	let (neovim, sarthi) = split(config)?;
	// External after this call: as given, or a float if given `relative`, or as it is
	let external = match (get(&neovim, "external"), get(&neovim, "relative")) {
		(Some(external), _) => truthy(external),
		(None, None) => is_external(ctx, win)?,
		(None, Some(relative)) => relative.as_str() == Some("") && is_external(ctx, win)?,
	};
	let options =
		check(ctx, sarthi.as_ref(), map(ctx.nvim.win_get_var(win, "sarthi").map_err(neovim_error)?), external)?;
	if !neovim.is_empty() {
		ctx.nvim.win_set_config(win, Value::Map(neovim)).map_err(neovim_error)?;
	}

	let mut given = ctx.given().get(&win).cloned().unwrap_or_default();
	if let Some(sarthi) = &sarthi {
		given.place = field(sarthi, "place").or(given.place);
		given.font = field(sarthi, "font").or(given.font);
	}
	// No longer external: its place and fonts no longer apply.
	if !external {
		given = Given::default();
	}
	let place = resolve(ctx, given.place.as_ref())?;
	apply(ctx, win, given, place, options)
}

/// A window's config like `nvim_win_get_config()`, with its `sarthi` table: its `w:sarthi`, and what else was given
pub(super) fn get_config(ctx: &Ctx, win: Window) -> Result<Value, String> {
	let win = or_current(ctx, win)?;
	let mut config = map(ctx.nvim.win_get_config(win).map_err(neovim_error)?);
	let mut sarthi = map(ctx.nvim.win_get_var(win, "sarthi").map_err(neovim_error)?);
	// Not something to give back: the API sets it
	sarthi.retain(|(key, _)| key.as_str() != Some("managed"));
	let given = ctx.given().get(&win).cloned().unwrap_or_default();
	for (key, value) in [("place", given.place), ("font", given.font)] {
		if let Some(value) = value {
			insert(&mut sarthi, key, value);
		}
	}
	if !sarthi.is_empty() {
		insert(&mut config, "sarthi", Value::Map(sarthi));
	}
	Ok(Value::Map(config))
}

/// Gives `win` its `w:sarthi`, `options`, and the place and fonts `given`, the place resolved to `place`; or what's
/// wrong with them
fn apply(ctx: &Ctx, win: Window, given: Given, place: Option<WindowPlace>, options: Value) -> Result<(), String> {
	ctx.nvim.win_set_var(win, "sarthi", options).map_err(neovim_error)?;
	// At once, so sarthi has the window's style before Neovim draws it
	ctx.nvim
		.exec_lua::<()>("require('sarthi.floats').send(...)", (win.0,))
		.map_err(neovim_error)?;

	let place = place::place(place.as_ref()).map_err(|mistake| format!("`sarthi.place`: {mistake}"))?;
	let font = given
		.font
		.clone()
		.map(decode::<WindowFont>)
		.transpose()
		.map_err(|mistake| format!("`sarthi.font`: {mistake}"))?;
	ctx.on_main(move |app| {
		app.editor.set_external_place(win, place);
		app.refresh();
		app.set_window_font(win, font)
	})?
	.map_err(|mistake| format!("`sarthi.font`: {mistake}"))?;
	ctx.given().insert(win, given);
	Ok(())
}

/// The `w:sarthi` of a window that is `external` or not: `options`, its `w:sarthi` until now, with what `sarthi` gives
/// merged in, key by key, and marked `managed`; or what's wrong with either. Unchanged without a `sarthi` table.
fn check(ctx: &Ctx, sarthi: Option<&Value>, options: Table, external: bool) -> Result<Value, String> {
	let Some(sarthi) = sarthi else { return Ok(Value::Map(options)) };
	let Some(sarthi) = as_map(sarthi) else {
		return Err(format!("`sarthi` takes a table, not a {}", lua_type(sarthi)));
	};
	for (key, value) in sarthi {
		let key = key.as_str().unwrap_or_default();
		if !OPTIONS.contains(&key) && !EXTERNAL_ONLY.iter().any(|(only, _)| *only == key) {
			return Err(format!("`sarthi` has no `{key}`"));
		}
		if let Some((_, why)) = EXTERNAL_ONLY.iter().find(|(only, _)| *only == key)
			&& !external
			&& !value.is_nil()
		{
			return Err(format!("`sarthi.{key}` needs `external = true`: {why}"));
		}
		if as_map(value).is_none() {
			return Err(format!("`sarthi.{key}` takes a table, not a {}", lua_type(value)));
		}
	}

	let mut merged = options;
	insert(&mut merged, "managed", Value::Boolean(true));
	for section in OPTIONS {
		if let Some(given) = get(sarthi, section).and_then(as_map) {
			let mut values = get(&merged, section).and_then(as_map).cloned().unwrap_or_default();
			for (key, value) in given {
				insert(&mut values, key.as_str().unwrap_or_default(), value.clone());
			}
			insert(&mut merged, section, Value::Map(values));
		}
	}

	// Sarthi decodes and checks the options, against the settings they go over.
	let merged = Value::Map(merged);
	let options: WindowOptions = decode(merged.clone()).map_err(|mistake| format!("`sarthi`: {mistake}"))?;
	ctx.on_main(move |app| app.settings.window.with(&options).validate())?
		.map_err(|mistake| format!("`sarthi`: {mistake}"))?;
	Ok(merged)
}

/// `place` with what it's relative to as it is now, before a window is opened and maybe entered: the current window
/// for `'win'` without one, and the cursor for `'cursor'`
fn resolve(ctx: &Ctx, place: Option<&Value>) -> Result<Option<WindowPlace>, String> {
	let Some(place) = place.filter(|place| !place.is_nil()) else { return Ok(None) };
	let mut place: WindowPlace = decode(place.clone()).map_err(|mistake| format!("`sarthi.place`: {mistake}"))?;
	match place.relative {
		Some(PlaceRelative::Win) if place.win.is_none_or(|win| win.0 == 0) => {
			place.win = Some(ctx.nvim.current_win().map_err(neovim_error)?);
		},
		// By the cursor where it is now, which sarthi only learns with Neovim's next redraw: relative to the current
		// window, at the cursor's cell of it, so in its own cells if it's an external window with fonts of its own
		Some(PlaceRelative::Cursor) => {
			let win = ctx.nvim.current_win().map_err(neovim_error)?;
			let line: f64 = ctx.nvim.call_function("winline", Vec::new()).map_err(neovim_error)?;
			let col: f64 = ctx.nvim.call_function("wincol", Vec::new()).map_err(neovim_error)?;
			place.relative = Some(PlaceRelative::Win);
			place.win = Some(win);
			place.row = Some(line - 1.0 + place.row.unwrap_or(0.0));
			place.col = Some(col - 1.0 + place.col.unwrap_or(0.0));
		},
		_ => (),
	}
	Ok(Some(place))
}

/// `win`, or the current window for 0
fn or_current(ctx: &Ctx, win: Window) -> Result<Window, String> {
	if win.0 == 0 { ctx.nvim.current_win().map_err(neovim_error) } else { Ok(win) }
}

/// Whether `win` is an external window now
fn is_external(ctx: &Ctx, win: Window) -> Result<bool, String> {
	let config = map(ctx.nvim.win_get_config(win).map_err(neovim_error)?);
	Ok(get(&config, "external").is_some_and(truthy))
}

/// `config` without its `sarthi` table, for Neovim, and the table
fn split(config: Value) -> Result<(Table, Option<Value>), String> {
	let Some(mut config) = into_map(config.clone()) else {
		return Err(format!("the config takes a table, not a {}", lua_type(&config)));
	};
	let sarthi = config
		.iter()
		.position(|(key, _)| key.as_str() == Some("sarthi"))
		.map(|index| config.remove(index).1);
	Ok((config, sarthi.filter(|sarthi| !sarthi.is_nil())))
}

/// A field of table `value`, unless it's left out or nil
fn field(value: &Value, key: &str) -> Option<Value> {
	as_map(value)
		.and_then(|map| get(map, key))
		.filter(|value| !value.is_nil())
		.cloned()
}

/// `value` decoded as `T`, or serde's message for why it isn't one. Through msgpack, like the requests: unlike
/// decoding from a value, it reads enums from their names, e.g. `relative = 'editor'`.
fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
	let mut bytes = Vec::new();
	rmpv::encode::write_value(&mut bytes, &value).map_err(|error| error.to_string())?;
	rmp_serde::from_slice(&bytes).map_err(|error| match error {
		rmp_serde::decode::Error::Syntax(message) => message,
		// Decoded again, through a value, for serde's message, e.g. "invalid type: string, expected f32", rather than
		// rmp_serde's about msgpack markers, like the reader does for requests
		error => match rmpv::ext::from_value::<T>(value) {
			Err(rmpv::ext::Error::Syntax(message)) => message,
			Ok(_) => error.to_string(),
		},
	})
}

/// Lua's tables arrive as maps, and empty ones as empty arrays.
fn as_map(value: &Value) -> Option<&Table> {
	static EMPTY: Table = Vec::new();
	match value {
		Value::Map(map) => Some(map),
		Value::Array(array) if array.is_empty() => Some(&EMPTY),
		_ => None,
	}
}

/// Like [`as_map`], owned
fn into_map(value: Value) -> Option<Table> {
	match value {
		Value::Map(map) => Some(map),
		Value::Array(array) if array.is_empty() => Some(Vec::new()),
		_ => None,
	}
}

/// A table that Neovim returned, or an empty one for nil (e.g. a window without `w:sarthi`)
fn map(value: Value) -> Table {
	into_map(value).unwrap_or_default()
}

fn get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
	map.iter().find(|(name, _)| name.as_str() == Some(key)).map(|(_, value)| value)
}

/// Sets `key` of `map` to `value`, in place of its value if it has one
fn insert(map: &mut Table, key: &str, value: Value) {
	match map.iter_mut().find(|(name, _)| name.as_str() == Some(key)) {
		Some((_, old)) => *old = value,
		None => map.push((Value::from(key), value)),
	}
}

/// Whether Lua takes `value` as true: anything but `false` and nil
fn truthy(value: &Value) -> bool {
	!matches!(value, Value::Boolean(false) | Value::Nil)
}

/// The Lua type of `value`, for messages
fn lua_type(value: &Value) -> &'static str {
	match value {
		Value::Nil => "nil",
		Value::Boolean(_) => "boolean",
		Value::Integer(_) | Value::F32(_) | Value::F64(_) => "number",
		Value::String(_) | Value::Binary(_) => "string",
		Value::Array(_) | Value::Map(_) => "table",
		Value::Ext(..) => "userdata",
	}
}
