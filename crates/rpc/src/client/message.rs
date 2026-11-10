use crate::{Notification, Request};

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum OutgoingRpcMessage<T> {
	Request(RpcRequest<T>),

	/// A call Neovim doesn't reply to. It reports errors from such calls with `nvim_error_event`.
	Notification {
		method: &'static str,
		params: T,
	},

	/// Successful response to a request from the server
	Response {
		id: u32,
		result: T,
	},

	/// Failed response to a request from the server
	ErrorResponse {
		id: u32,
		message: String,
	},
}

/// A message from the server: a request from nvui's Lua, a response with a result of type `T`, or a
/// notification
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum IncomingRpcMessage<T> {
	Request { id: u32, request: Request },
	Response(RpcResponse<T>),
	Notification(Notification),
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct RpcRequest<P> {
	pub id: u32,
	pub method: String,
	pub params: P,
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct RpcResponse<R> {
	pub id: u32,
	pub result: Result<R, rmpv::Value>,
}

mod ser_de {
	use std::marker::PhantomData;

	use serde::{
		Deserialize, Deserializer, Serialize,
		de::{Error as DeError, SeqAccess, Visitor, value::SeqAccessDeserializer},
		ser::SerializeTuple,
	};

	use super::*;

	impl<T> Serialize for OutgoingRpcMessage<T>
	where
		T: Serialize,
	{
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: serde::Serializer,
		{
			match self {
				Self::Request(req) => {
					let mut tuple = serializer.serialize_tuple(4)?;
					tuple.serialize_element(&0)?;
					tuple.serialize_element(&req.id)?;
					tuple.serialize_element(&req.method)?;
					tuple.serialize_element(&req.params)?;
					tuple.end()
				},
				Self::Notification { method, params } => {
					let mut tuple = serializer.serialize_tuple(3)?;
					tuple.serialize_element(&2)?;
					tuple.serialize_element(method)?;
					tuple.serialize_element(params)?;
					tuple.end()
				},
				Self::Response { id, result } => {
					let mut tuple = serializer.serialize_tuple(4)?;
					tuple.serialize_element(&1)?;
					tuple.serialize_element(id)?;
					tuple.serialize_element(&())?;
					tuple.serialize_element(result)?;
					tuple.end()
				},
				Self::ErrorResponse { id, message } => {
					// Errors use Neovim's `[type, message]` shape, with type 0 (Exception).
					let mut tuple = serializer.serialize_tuple(4)?;
					tuple.serialize_element(&1)?;
					tuple.serialize_element(id)?;
					tuple.serialize_element(&(0, message))?;
					tuple.serialize_element(&())?;
					tuple.end()
				},
			}
		}
	}

	struct IncomingRpcMessageVisitor<T>(PhantomData<T>);

	impl<'de, T> Visitor<'de> for IncomingRpcMessageVisitor<T>
	where
		T: Deserialize<'de>,
	{
		type Value = IncomingRpcMessage<T>;

		fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
			formatter.write_str("a msgpack rpc tuple")
		}

		fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
		where
			A: SeqAccess<'de>,
		{
			let msg_type = seq.next_element::<u8>()?.ok_or_else(|| DeError::invalid_length(0, &self))?;

			match msg_type {
				0 => {
					let id = seq.next_element::<u32>()?.ok_or_else(|| DeError::invalid_length(1, &self))?;

					// The method and its arguments, like a notification's
					let request = Request::deserialize(SeqAccessDeserializer::new(seq))?;
					Ok(IncomingRpcMessage::Request { id, request })
				},
				1 => {
					let id = seq.next_element::<u32>()?.ok_or_else(|| DeError::invalid_length(1, &self))?;

					let error = seq
						.next_element::<rmpv::Value>()?
						.ok_or_else(|| DeError::invalid_length(2, &self))?;

					if error == rmpv::Value::Nil {
						let result = seq.next_element::<T>()?.ok_or_else(|| DeError::invalid_length(3, &self))?;

						Ok(IncomingRpcMessage::Response(RpcResponse { id, result: Ok(result) }))
					} else {
						let _ = seq
							.next_element::<rmpv::Value>()?
							.ok_or_else(|| DeError::invalid_length(3, &self))?;

						Ok(IncomingRpcMessage::Response(RpcResponse { id, result: Err(error) }))
					}
				},
				2 => {
					let d = SeqAccessDeserializer::new(seq);
					Notification::deserialize(d).map(IncomingRpcMessage::Notification)
				},
				v => Err(DeError::unknown_variant(v.to_string().as_str(), &["0", "1", "2"])),
			}
		}
	}

	impl<'de, T> Deserialize<'de> for IncomingRpcMessage<T>
	where
		T: Deserialize<'de>,
	{
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: Deserializer<'de>,
		{
			deserializer.deserialize_seq(IncomingRpcMessageVisitor(PhantomData))
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{GetClipboardArgs, GridResizeEvent, RedrawNotification, Register};

	use super::*;

	#[test]
	fn ser_de_request() {
		type Params = (u32, String, Vec<String>);

		let req = OutgoingRpcMessage::<Params>::Request(RpcRequest {
			id: 10,
			method: String::from("test"),
			params: (1, String::from("abc"), vec![String::from("def")]),
		});

		let req_bytes = vec![
			0x94, 0x00, 0x0A, 0xA4, 0x74, 0x65, 0x73, 0x74, 0x93, 0x01, 0xA3, 0x61, 0x62, 0x63, 0x91, 0xA3, 0x64, 0x65,
			0x66,
		];

		let req_serialized = rmp_serde::to_vec_named(&req).unwrap();

		assert_eq!(req_serialized, req_bytes);
	}

	#[test]
	fn de_request() {
		// [0, 10, "get_clipboard", ["+"]]: the method and arguments decode straight into nvui's request
		let request = [&[0x94, 0x00, 0x0A, 0xAD][..], b"get_clipboard", &[0x91, 0xA1, b'+']].concat();
		let request: IncomingRpcMessage<()> = rmp_serde::from_slice(&request).unwrap();
		let expected = Request::GetClipboard(GetClipboardArgs { register: Register::Clipboard });
		assert_eq!(request, IncomingRpcMessage::Request { id: 10, request: expected });

		// [0, 11, "other", []]
		let other = [&[0x94, 0x00, 0x0B, 0xA5][..], b"other", &[0x90]].concat();
		let other: IncomingRpcMessage<()> = rmp_serde::from_slice(&other).unwrap();
		let expected = Request::Other { method: String::from("other"), value: serde::de::IgnoredAny };
		assert_eq!(other, IncomingRpcMessage::Request { id: 11, request: expected });
	}

	#[test]
	fn ser_notification() {
		let notification = OutgoingRpcMessage::Notification { method: "nvim_input", params: ["ix"] };

		// [2, "nvim_input", ["ix"]]
		let expected =
			vec![0x93, 0x02, 0xAA, b'n', b'v', b'i', b'm', b'_', b'i', b'n', b'p', b'u', b't', 0x91, 0xA2, b'i', b'x'];

		assert_eq!(rmp_serde::to_vec_named(&notification).unwrap(), expected);
	}

	#[test]
	fn ser_error_response() {
		let response = OutgoingRpcMessage::<()>::ErrorResponse { id: 3, message: String::from("x") };

		// [1, 3, [0, "x"], nil]
		let expected = vec![0x94, 0x01, 0x03, 0x92, 0x00, 0xA1, b'x', 0xC0];

		assert_eq!(rmp_serde::to_vec_named(&response).unwrap(), expected);
	}

	#[test]
	fn ser_response() {
		let response = OutgoingRpcMessage::Response { id: 3, result: 42 };

		// [1, 3, nil, 42]
		let expected = vec![0x94, 0x01, 0x03, 0xC0, 0x2A];

		assert_eq!(rmp_serde::to_vec_named(&response).unwrap(), expected);
	}

	#[test]
	fn de_notification() {
		let expected = IncomingRpcMessage::<()>::Notification(Notification::Redraw(vec![
			RedrawNotification::GridResize(vec![GridResizeEvent { grid: 1, width: 10, height: 10 }]),
		]));

		let notification = vec![
			0x93, 0x02, 0xA6, 0x72, 0x65, 0x64, 0x72, 0x61, 0x77, 0x91, 0x92, 0xAB, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x72,
			0x65, 0x73, 0x69, 0x7A, 0x65, 0x93, 0x01, 0x0A, 0x0A,
		];

		let deserialized: IncomingRpcMessage<()> = rmp_serde::from_slice(&notification).unwrap();

		assert_eq!(deserialized, expected);
	}
}
