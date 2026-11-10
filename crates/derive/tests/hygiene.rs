//! The generated code must compile next to names it could otherwise clash with.

#![allow(dead_code, non_camel_case_types)]

/// A local `Result` alias, as many crates define
type Result<T> = core::result::Result<T, ()>;

/// Shadow prelude items the generated code uses.
struct Ok;
struct Some;
struct Option;
struct Vec;

/// Types named like the generic parameters the generated code declares
#[derive(Debug, PartialEq, serde::Deserialize)]
struct A(u32);
#[derive(Debug, PartialEq, serde::Deserialize)]
struct D(u32);
#[derive(Debug, PartialEq, serde::Deserialize)]
struct T(u32);
#[derive(Debug, PartialEq, serde::Deserialize)]
struct E(u32);

/// A type named like a visitor the generated code could have declared
struct EventVisitor;
struct Tag;

#[derive(Debug, PartialEq, sarthi_derive::DeserializeTaggedEnum)]
enum Event {
	First(#[tagged_enum(flatten)] std::vec::Vec<A>),
	Second(D, T),
	Third { e: E },
	Other { method: String, value: rmpv::Value },
}

#[test]
fn compiles_and_decodes() {
	let mut bytes = std::vec::Vec::new();
	let first = rmpv::Value::Array(vec!["first".into(), rmpv::Value::Array(vec![1.into()])]);
	rmpv::encode::write_value(&mut bytes, &first).unwrap();

	let event: core::result::Result<Event, _> = rmp_serde::from_slice(&bytes);
	assert_eq!(event.unwrap(), Event::First(vec![A(1)]));
}
