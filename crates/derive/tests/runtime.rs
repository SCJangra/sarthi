//! Decoding behaviour of `DeserializeTaggedEnum`, using msgpack as Neovim sends it.

use nvui_derive::DeserializeTaggedEnum;
use rmpv::Value;
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
struct Pos {
	row: u32,
	col: u32,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Resize {
	width: u32,
	#[serde(default)]
	height: Option<u32>,
}

#[derive(Debug, PartialEq, DeserializeTaggedEnum)]
enum Event {
	/// One element per occurrence, like most redraw events
	CursorGoto(#[tagged_enum(flatten)] Vec<Pos>),

	/// Plain fields read in order
	Title(String),

	/// Named fields read in order, then the occurrences
	Named {
		grid: u32,
		#[tagged_enum(flatten)]
		resizes: Vec<Resize>,
	},

	/// No parameters, like `flush`
	Flush,

	Other {
		method: String,
		#[tagged_enum(flatten)]
		value: Vec<Value>,
	},
}

/// A fallback that keeps its data as one value, like `NvimNotification`
#[derive(Debug, PartialEq, DeserializeTaggedEnum)]
enum Notification {
	Redraw(Vec<Event>),

	Other { method: String, value: Value },
}

fn encode(value: Value) -> Vec<u8> {
	let mut bytes = Vec::new();
	rmpv::encode::write_value(&mut bytes, &value).unwrap();
	bytes
}

fn arr(values: Vec<Value>) -> Value {
	Value::Array(values)
}

fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, rmp_serde::decode::Error> {
	rmp_serde::from_slice(bytes)
}

#[test]
fn flattened_occurrences() {
	let bytes = encode(arr(vec!["cursor_goto".into(), arr(vec![1.into(), 2.into()]), arr(vec![3.into(), 4.into()])]));
	assert_eq!(
		decode::<Event>(&bytes).unwrap(),
		Event::CursorGoto(vec![Pos { row: 1, col: 2 }, Pos { row: 3, col: 4 }])
	);
}

#[test]
fn flattened_without_occurrences() {
	let bytes = encode(arr(vec!["cursor_goto".into()]));
	assert_eq!(decode::<Event>(&bytes).unwrap(), Event::CursorGoto(vec![]));
}

#[test]
fn flattened_ignores_trailing_arguments() {
	let bytes = encode(arr(vec![
		"cursor_goto".into(),
		arr(vec![1.into(), 2.into(), "appended".into(), arr(vec![])]),
		arr(vec![3.into(), 4.into()]),
	]));
	assert_eq!(
		decode::<Event>(&bytes).unwrap(),
		Event::CursorGoto(vec![Pos { row: 1, col: 2 }, Pos { row: 3, col: 4 }])
	);
}

#[test]
fn flattened_with_optional_argument_missing() {
	let bytes = encode(arr(vec!["named".into(), 7.into(), arr(vec![10.into(), 20.into()]), arr(vec![30.into()])]));
	let expected = Event::Named {
		grid: 7,
		resizes: vec![Resize { width: 10, height: Some(20) }, Resize { width: 30, height: None }],
	};
	assert_eq!(decode::<Event>(&bytes).unwrap(), expected);
}

#[test]
fn flattened_element_must_be_a_sequence() {
	let bytes = encode(arr(vec!["cursor_goto".into(), 5.into()]));
	let error = decode::<Event>(&bytes).unwrap_err().to_string();
	assert_eq!(error, "invalid type: integer `5`, expected a sequence of arguments");
}

#[test]
fn plain_field() {
	let bytes = encode(arr(vec!["title".into(), "nvui".into()]));
	assert_eq!(decode::<Event>(&bytes).unwrap(), Event::Title(String::from("nvui")));
}

#[test]
fn plain_field_missing() {
	let bytes = encode(arr(vec!["title".into()]));
	let error = decode::<Event>(&bytes).unwrap_err().to_string();
	assert_eq!(error, "invalid length 1, expected a tagged enum sequence");
}

#[test]
fn unit_variant_skips_its_empty_argument_list() {
	let bytes = encode(arr(vec!["flush".into(), arr(vec![])]));
	assert_eq!(decode::<Event>(&bytes).unwrap(), Event::Flush);
}

#[test]
fn unit_variant_without_argument_list() {
	let bytes = encode(arr(vec!["flush".into()]));
	assert_eq!(decode::<Event>(&bytes).unwrap(), Event::Flush);
}

#[test]
fn unit_variant_skips_unexpected_arguments() {
	let bytes = encode(arr(vec!["flush".into(), arr(vec![1.into()]), arr(vec![])]));
	assert_eq!(decode::<Event>(&bytes).unwrap(), Event::Flush);
}

#[test]
fn unknown_tag_goes_to_other() {
	let bytes = encode(arr(vec!["mode_change".into(), arr(vec!["normal".into(), 0.into()]), 5.into()]));
	let expected = Event::Other {
		method: String::from("mode_change"),
		value: vec![arr(vec!["normal".into(), 0.into()]), 5.into()],
	};
	assert_eq!(decode::<Event>(&bytes).unwrap(), expected);
}

#[test]
fn other_with_a_single_value() {
	let bytes = encode(arr(vec!["nvui_custom".into(), arr(vec![1.into(), "two".into()])]));
	let expected =
		Notification::Other { method: String::from("nvui_custom"), value: arr(vec![1.into(), "two".into()]) };
	assert_eq!(decode::<Notification>(&bytes).unwrap(), expected);
}

#[test]
fn nested_enums() {
	let bytes = encode(arr(vec![
		"redraw".into(),
		arr(vec![
			arr(vec!["cursor_goto".into(), arr(vec![1.into(), 2.into()])]),
			arr(vec!["flush".into(), arr(vec![])]),
		]),
	]));
	let expected = Notification::Redraw(vec![Event::CursorGoto(vec![Pos { row: 1, col: 2 }]), Event::Flush]);
	assert_eq!(decode::<Notification>(&bytes).unwrap(), expected);
}

#[test]
fn tag_as_bin() {
	let bytes = encode(arr(vec![Value::Binary(b"flush".to_vec()), arr(vec![])]));
	assert_eq!(decode::<Event>(&bytes).unwrap(), Event::Flush);
}

#[test]
fn unknown_tag_as_bin() {
	let bytes = encode(arr(vec![Value::Binary(b"busy_start".to_vec()), arr(vec![])]));
	let expected = Event::Other { method: String::from("busy_start"), value: vec![arr(vec![])] };
	assert_eq!(decode::<Event>(&bytes).unwrap(), expected);
}

#[test]
fn tag_as_bin_that_is_not_utf8() {
	let bytes = encode(arr(vec![Value::Binary(vec![0x66, 0xFF]), arr(vec![])]));
	let error = decode::<Event>(&bytes).unwrap_err().to_string();
	assert_eq!(error, "invalid value: byte array, expected a string");
}

#[test]
fn tag_that_is_not_a_string() {
	let bytes = encode(arr(vec![42.into(), arr(vec![])]));
	let error = decode::<Event>(&bytes).unwrap_err().to_string();
	assert_eq!(error, "invalid type: integer `42`, expected a string");
}

#[test]
fn tag_missing() {
	let bytes = encode(arr(vec![]));
	let error = decode::<Event>(&bytes).unwrap_err().to_string();
	assert_eq!(error, "invalid length 0, expected a tagged enum sequence");
}

#[test]
fn not_a_sequence() {
	let bytes = encode("flush".into());
	let error = decode::<Event>(&bytes).unwrap_err().to_string();
	assert_eq!(error, "invalid type: string \"flush\", expected a tagged enum sequence");
}

#[test]
fn tag_matching_is_exact() {
	// Only the exact snake-case name selects a variant.
	for tag in ["Flush", "FLUSH", "flush ", "cursorGoto", "cursor-goto"] {
		let bytes = encode(arr(vec![tag.into()]));
		let expected = Event::Other { method: String::from(tag), value: vec![] };
		assert_eq!(decode::<Event>(&bytes).unwrap(), expected, "tag {tag:?}");
	}
}

#[test]
fn many_occurrences() {
	let occurrences = (0..10_000u32).map(|i| arr(vec![i.into(), (i + 1).into()]));
	let bytes = encode(arr(std::iter::once("cursor_goto".into()).chain(occurrences).collect()));
	let Event::CursorGoto(positions) = decode::<Event>(&bytes).unwrap() else { panic!("wrong variant") };
	assert_eq!(positions.len(), 10_000);
	assert_eq!(positions[9_999], Pos { row: 9_999, col: 10_000 });
}

/// `method` is filled from the tag, so a flattened `value` may come before it.
#[derive(Debug, PartialEq, DeserializeTaggedEnum)]
enum ValueFirst {
	Flush,

	Other {
		#[tagged_enum(flatten)]
		value: Vec<Value>,
		method: String,
	},
}

#[test]
fn other_with_flattened_value_before_method() {
	let bytes = encode(arr(vec!["busy_start".into(), arr(vec![]), 1.into()]));
	let expected = ValueFirst::Other { value: vec![arr(vec![]), 1.into()], method: String::from("busy_start") };
	assert_eq!(decode::<ValueFirst>(&bytes).unwrap(), expected);
}

#[test]
fn other_value_missing() {
	// `method` comes from the tag, so `value` is element 1.
	let bytes = encode(arr(vec!["nvui_custom".into()]));
	let error = decode::<Notification>(&bytes).unwrap_err().to_string();
	assert_eq!(error, "invalid length 1, expected a tagged enum sequence");
}
