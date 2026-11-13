//! sarthi's development tasks. `cargo xtask docs` writes the help file, `runtime/doc/sarthi.txt`, from
//! `runtime/doc/sarthi.txt.in`: its `@settings@` from the settings as declared (`rpc::sarthi::SECTIONS`), with their
//! defaults as serde gives them for an empty table, and its `@lua@` from the Lua modules' annotations, by vimcats.

use std::fmt::Write;
use std::path::Path;
use std::process::Command;

use rmpv::Value;
use rpc::sarthi::{Animation, SECTIONS, SectionDoc, Settings};

/// Width of the help file's text
const WIDTH: usize = 78;

/// The Lua modules whose annotations go in the help file, in order
const LUA: [&str; 2] = ["runtime/lua/sarthi/init.lua", "runtime/lua/sarthi/api.lua"];

fn main() {
	match std::env::args().nth(1).as_deref() {
		Some("docs") => docs(),
		_ => {
			eprintln!("usage: cargo xtask docs");
			std::process::exit(2);
		},
	}
}

fn docs() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let template = std::fs::read_to_string(root.join("runtime/doc/sarthi.txt.in")).expect("reading the template");
	let help = template
		.replace("@settings@\n", &settings())
		.replace("@lua@\n", &functions(&root));
	std::fs::write(root.join("runtime/doc/sarthi.txt"), help).expect("writing the help file");
}

/// The settings' sections and the curves of animations, with each key's type and default
fn settings() -> String {
	// What serde gives for an empty table
	let empty = Value::Map(Vec::new());
	let defaults: Settings = rmpv::ext::from_value(empty).expect("the defaults decode");
	let Value::Map(defaults) = named(&defaults) else { unreachable!() };

	let mut out = String::new();
	for section in SECTIONS {
		let (_, values) = defaults
			.iter()
			.find(|(name, _)| name.as_str() == Some(section.name))
			.expect("a section");
		write_section(&mut out, section, values);
	}
	write_curves(&mut out);
	out
}

fn write_section(out: &mut String, section: &SectionDoc, values: &Value) {
	let tag = format!("sarthi-{}", section.name);
	heading(out, &section.name.to_uppercase(), &tag);
	paragraph(out, &join(section.doc), "");
	if !section.window_keys.is_empty() {
		let keys = section
			.window_keys
			.iter()
			.map(|key| format!("`{key}`"))
			.collect::<Vec<_>>()
			.join(", ");
		let text = format!(
			"Per window too, in `w:sarthi.{0}` or `sarthi.{0}` (|sarthi.api|): {keys}. Each is the window's, whatever \
			 the setting's, but `exclude_filetypes` wins over a window's `enabled`.",
			section.name
		);
		paragraph(out, &text, "");
	}
	for key in section.keys {
		let Value::Map(values) = values else { unreachable!() };
		let (_, default) = values
			.iter()
			.find(|(name, _)| name.as_str() == Some(key.name))
			.expect("a default");
		let name = format!("{}.{}", section.name, key.name);
		heading_line(out, &name, &format!("sarthi-{name}"));
		let lua_type = if key.lua_type.is_empty() { lua_type(key.rust_type) } else { key.lua_type.to_owned() };
		let _ = writeln!(out, "    Type: {lua_type}\n    Default: `{}`", lua(default));
		paragraph(out, &join(key.doc), "    ");
	}
}

/// The curves an animation (`{ curve = …, … }`) can have, each with its defaults
fn write_curves(out: &mut String) {
	heading(out, "ANIMATIONS", "sarthi-animation");
	paragraph(
		out,
		"An animation is a table with its `curve` and that curve's values, in milliseconds for durations. A value left out has the default shown.",
		"",
	);
	for curve in Animation::CURVES {
		let table = format!(
			"{{ curve = '{}'{} }}",
			curve.name,
			if curve.required.is_empty() { String::new() } else { format!(", {}", curve.required) }
		);
		let animation = decode_curve(curve.name, curve.required);
		assert_eq!(animation.curve(), curve.name, "Animation::CURVES lists each curve once, in its name");
		let full = named(&animation);
		heading_line(out, &format!("'{}'", curve.name), &format!("sarthi-animation-{}", curve.name));
		let Value::Map(full) = full else { unreachable!() };
		// What it has besides `curve` and what's required: its defaults
		let defaults: Vec<String> = full
			.iter()
			.filter_map(|(key, value)| Some((key.as_str()?, value)))
			.filter(|(key, _)| *key != "curve" && !curve.required.starts_with(&format!("{key} =")))
			.map(|(key, value)| format!("{key} = {}", lua(value)))
			.collect();
		let _ = writeln!(out, "    `{table}`");
		if !defaults.is_empty() {
			let _ = writeln!(out, "    Defaults: `{}`", defaults.join(", "));
		}
		paragraph(out, curve.doc, "    ");
	}
}

/// The animation `{ curve = name, required }` decodes to, the rest at its defaults
fn decode_curve(name: &str, required: &str) -> Animation {
	let mut pairs = vec![(Value::from("curve"), Value::from(name))];
	// The only required value is `points`, a list of numbers.
	if let Some(points) = required.strip_prefix("points = ") {
		let numbers = points
			.trim_matches(['{', '}', ' '])
			.split(", ")
			.map(|n| Value::from(n.parse::<f64>().unwrap()));
		pairs.push((Value::from("points"), Value::Array(numbers.collect())));
	}
	rmpv::ext::from_value(Value::Map(pairs)).expect("a curve decodes")
}

/// The Lua functions, from their annotations
fn functions(root: &Path) -> String {
	let output = Command::new("vimcats")
		.args(["--no-modeline", "--prefix-func", "--layout", "compact:4"])
		.args(LUA.map(|path| root.join(path)))
		.output()
		.expect("running vimcats (in the dev shell)");
	assert!(output.status.success(), "vimcats: {}", String::from_utf8_lossy(&output.stderr));
	let text = String::from_utf8(output.stdout).expect("vimcats writes UTF-8");
	// vimcats puts tags at the right of 80 columns; the rest of the file has 78.
	let lines = text.lines().map(|line| {
		if line.ends_with('*') && line.chars().count() > WIDTH && line.contains("   *") {
			line.replacen("   *", " *", 1)
		} else {
			line.to_owned()
		}
	});
	lines.map(|line| line + "\n").collect()
}

/// `value` as a MessagePack value, with its structs' keys
fn named(value: &impl serde::Serialize) -> Value {
	let bytes = rmp_serde::to_vec_named(value).expect("encoding");
	rmpv::decode::read_value(&mut bytes.as_slice()).expect("decoding")
}

/// `value` as Lua
fn lua(value: &Value) -> String {
	match value {
		Value::Boolean(value) => value.to_string(),
		Value::F32(value) => value.to_string(),
		Value::F64(value) => value.to_string(),
		Value::Integer(value) => value.to_string(),
		Value::String(value) => format!("'{}'", value.as_str().unwrap_or_default()),
		Value::Array(values) if values.is_empty() => String::from("{}"),
		Value::Array(values) => format!("{{ {} }}", values.iter().map(lua).collect::<Vec<_>>().join(", ")),
		Value::Map(pairs) => {
			let pairs = pairs
				.iter()
				.map(|(key, value)| format!("{} = {}", key.as_str().unwrap_or_default(), lua(value)));
			format!("{{ {} }}", pairs.collect::<Vec<_>>().join(", "))
		},
		other => panic!("no Lua for {other}"),
	}
}

/// The Lua type of a key of Rust type `rust` (as `stringify!` writes it)
fn lua_type(rust: &str) -> String {
	match rust.replace(' ', "").as_str() {
		"bool" => "boolean",
		"f32" | "f64" | "u32" => "number",
		"String" => "string",
		"Vec<String>" => "string[]",
		"[f32;2]" => "{ number, number }",
		"Animation" => "|sarthi-animation|",
		other => panic!("no Lua type for `{other}`: give one with `as \"…\"` in its section!"),
	}
	.to_owned()
}

/// Doc comment lines as one text
fn join(lines: &[&str]) -> String {
	lines.iter().map(|line| line.trim()).collect::<Vec<_>>().join(" ")
}

/// A section's heading: a rule, then `title` with `*tag*` on the right
fn heading(out: &mut String, title: &str, tag: &str) {
	let _ = writeln!(out, "{}", "-".repeat(WIDTH));
	heading_line(out, title, tag);
	out.push('\n');
}

/// `title` with `*tag*` on the right
fn heading_line(out: &mut String, title: &str, tag: &str) {
	let tag = format!("*{tag}*");
	let gap = WIDTH.saturating_sub(title.chars().count() + tag.chars().count()).max(1);
	let _ = writeln!(out, "{title}{}{tag}", " ".repeat(gap));
}

/// `text` wrapped to the width after `indent`, then a blank line
fn paragraph(out: &mut String, text: &str, indent: &str) {
	let mut line = String::new();
	for word in text.split_whitespace() {
		if !line.is_empty() && indent.len() + line.chars().count() + 1 + word.chars().count() > WIDTH {
			let _ = writeln!(out, "{indent}{line}");
			line.clear();
		}
		if !line.is_empty() {
			line.push(' ');
		}
		line.push_str(word);
	}
	if !line.is_empty() {
		let _ = writeln!(out, "{indent}{line}");
	}
	out.push('\n');
}
