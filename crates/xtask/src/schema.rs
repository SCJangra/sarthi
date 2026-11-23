//! Reading the JSON schemas `schemars` makes of the settings and the API: names, docs, Lua types and Lua values.

use serde_json::{Map, Value};

/// A schema and the definitions its `$ref`s point into.
pub struct Schema {
	pub root: Value,
	defs: Map<String, Value>,
}

impl Schema {
	/// The schema of `T`.
	pub fn of<T: schemars::JsonSchema>() -> Self {
		let root = schemars::schema_for!(T).to_value();
		let defs = root.get("$defs").and_then(Value::as_object).cloned().unwrap_or_default();
		Self { root, defs }
	}

	/// `schema` itself, or the definition it refers to.
	pub fn resolve<'a>(&'a self, schema: &'a Value) -> &'a Value {
		match schema.get("$ref").and_then(Value::as_str) {
			Some(reference) => self.def(name_of(reference)),
			None => schema,
		}
	}

	/// The definition named `name`.
	pub fn def(&self, name: &str) -> &Value {
		self.defs.get(name).unwrap_or_else(|| panic!("no definition of {name}"))
	}

	/// The definitions, by name.
	pub fn defs(&self) -> impl Iterator<Item = (&String, &Value)> {
		self.defs.iter()
	}
}

/// The name a `$ref` points to, e.g. `ScrollSettings` for `#/$defs/ScrollSettings`.
pub fn name_of(reference: &str) -> &str {
	reference.rsplit('/').next().unwrap_or(reference)
}

/// The properties of an object schema, in order, as (name, schema).
pub fn properties(schema: &Value) -> Vec<(&str, &Value)> {
	schema
		.get("properties")
		.and_then(Value::as_object)
		.map(|properties| properties.iter().map(|(name, schema)| (name.as_str(), schema)).collect())
		.unwrap_or_default()
}

/// Whether property `name` of an object schema must be given.
pub fn required(schema: &Value, name: &str) -> bool {
	schema
		.get("required")
		.and_then(Value::as_array)
		.is_some_and(|required| required.iter().any(|value| value.as_str() == Some(name)))
}

/// The doc comment of a schema, as Markdown: its paragraphs' lines joined, the paragraphs kept apart.
pub fn doc(schema: &Value) -> String {
	let text = schema.get("description").and_then(Value::as_str).unwrap_or_default();
	text.split("\n\n")
		.map(|paragraph| paragraph.split('\n').collect::<Vec<_>>().join(" "))
		.collect::<Vec<_>>()
		.join("\n\n")
}

/// The Lua type of a value of `schema`, as LuaCATS writes it, e.g. `number|false` or `sarthi.WindowPlace`.
pub fn lua_type(schema: &Value) -> String {
	if let Some(lua) = schema.get("x-lua-type").and_then(Value::as_str) {
		return lua.to_owned();
	}
	if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
		return named_type(name_of(reference));
	}
	if let Some(value) = schema.get("const") {
		return lua_value(value);
	}
	for key in ["anyOf", "oneOf"] {
		if let Some(choices) = schema.get(key).and_then(Value::as_array) {
			let mut types: Vec<String> = Vec::new();
			for choice in choices
				.iter()
				.filter(|choice| choice.get("type").and_then(Value::as_str) != Some("null"))
			{
				let lua = lua_type(choice);
				if !types.contains(&lua) {
					types.push(lua);
				}
			}
			return types.join("|");
		}
	}
	let types: Vec<&str> = match schema.get("type") {
		Some(Value::String(name)) => vec![name.as_str()],
		Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).filter(|name| *name != "null").collect(),
		_ => Vec::new(),
	};
	match types.as_slice() {
		["boolean"] => String::from("boolean"),
		["integer"] => String::from("integer"),
		["number"] => String::from("number"),
		["string"] => String::from("string"),
		["array"] => {
			let items = schema.get("items").map_or_else(|| String::from("any"), lua_type);
			if items.contains('|') { format!("({items})[]") } else { format!("{items}[]") }
		},
		["object"] => String::from("table"),
		_ => String::from("any"),
	}
}

/// The Lua type of definition `name`: its own type for the handles and lists, which are numbers and strings, and a
/// class of `sarthi` for the tables.
fn named_type(name: &str) -> String {
	match name {
		"Window" | "Buffer" => String::from("integer"),
		_ => format!("sarthi.{name}"),
	}
}

/// `value` as a Lua literal, e.g. `{ type = "spring", stiffness = 400 }`.
pub fn lua_value(value: &Value) -> String {
	match value {
		Value::Null => String::from("nil"),
		Value::Bool(value) => value.to_string(),
		Value::Number(number) => match number.as_f64() {
			Some(number) if number.fract() == 0.0 && number.abs() < 1e15 => format!("{number:.0}"),
			_ => number.to_string(),
		},
		Value::String(text) => format!("{text:?}"),
		Value::Array(items) if items.is_empty() => String::from("{}"),
		Value::Array(items) => format!("{{ {} }}", items.iter().map(lua_value).collect::<Vec<_>>().join(", ")),
		Value::Object(map) if map.is_empty() => String::from("{}"),
		Value::Object(map) => {
			let fields: Vec<String> = map.iter().map(|(key, value)| format!("{key} = {}", lua_value(value))).collect();
			format!("{{ {} }}", fields.join(", "))
		},
	}
}
