use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};

/// A Neovim window's handle (`:help window-ID`), like `1000`. Neovim sends it as msgpack extension type 1, wrapping the
/// number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Window(pub i64);

/// Neovim's extension type for window handles (`nvim_get_api_info()`'s `types`)
const WINDOW_EXT: i8 = 1;

/// The name rmp_serde and rmpv decode msgpack extensions through, as `(type, bytes)`
const EXT_STRUCT: &str = "_ExtStruct";

impl<'de> Deserialize<'de> for Window {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		deserializer.deserialize_newtype_struct(EXT_STRUCT, WindowVisitor)
	}
}

struct WindowVisitor;

impl<'de> Visitor<'de> for WindowVisitor {
	type Value = Window;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("a window handle")
	}

	/// What the decoders wrap an extension in
	fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<Window, D::Error> {
		deserializer.deserialize_any(self)
	}

	/// An extension: its type, then the handle encoded as msgpack
	fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Window, A::Error> {
		let ext: i8 = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
		let Bytes(bytes) = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
		if ext != WINDOW_EXT {
			return Err(de::Error::custom(format!("expected a window handle, got extension type {ext}")));
		}
		rmp::decode::read_int(&mut bytes.as_slice())
			.map(Window)
			.map_err(de::Error::custom)
	}
}

/// An extension's bytes, which the decoders hand over as bytes rather than as a sequence
struct Bytes(Vec<u8>);

impl<'de> Deserialize<'de> for Bytes {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		struct BytesVisitor;

		impl Visitor<'_> for BytesVisitor {
			type Value = Bytes;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("bytes")
			}

			fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Bytes, E> {
				Ok(Bytes(bytes.to_vec()))
			}

			fn visit_byte_buf<E: de::Error>(self, bytes: Vec<u8>) -> Result<Bytes, E> {
				Ok(Bytes(bytes))
			}
		}

		deserializer.deserialize_byte_buf(BytesVisitor)
	}
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::Window;

	fn decode(value: Value) -> (Result<Window, String>, Result<Window, String>) {
		let bytes = rmp_serde::to_vec(&value).unwrap();
		let rmp = rmp_serde::from_slice(&bytes).map_err(|err| err.to_string());
		let rmpv = rmpv::ext::from_value(value).map_err(|err| err.to_string());
		(rmp, rmpv)
	}

	#[test]
	fn from_neovim() {
		// Extension type 1 around 1000, encoded as a uint16
		let (rmp, rmpv) = decode(Value::Ext(1, vec![0xCD, 0x03, 0xE8]));
		assert_eq!((rmp, rmpv), (Ok(Window(1000)), Ok(Window(1000))));

		// A buffer handle (type 0) isn't a window.
		let (rmp, rmpv) = decode(Value::Ext(0, vec![0x01]));
		assert!(rmp.unwrap_err().contains("extension type 0"));
		assert!(rmpv.unwrap_err().contains("extension type 0"));
	}
}
