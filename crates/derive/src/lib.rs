mod tagged_enum;
mod utils;

use proc_macro::TokenStream;

/// Derives `serde::Deserialize` for an enum encoded as a sequence whose first element is a tag naming
/// the variant, followed by the variant's data: `[tag, element, …]`.
///
/// This is the shape of msgpack-rpc notifications (`[method, params]`, after the message type) and of
/// Neovim's redraw events (`[name, args, args, …]`, one argument list per occurrence of the event).
///
/// - The tag selects the variant whose name in snake case matches it (`GridLine` ↔ `"grid_line"`). It
///   may be a msgpack `str`, or `bin` holding UTF-8.
/// - Tuple and struct variants read their fields from the following elements, in order.
/// - A field marked `#[tagged_enum(flatten)]` must be a `Vec<T>` and the variant's last field; it collects
///   all remaining elements. In known variants each element must be a sequence (an argument list), and
///   elements after the ones `T` reads are skipped, since Neovim may append parameters to events.
/// - Unit variants skip all remaining elements, e.g. `["flush", []]`.
/// - The enum must have a fallback variant `Other { method: String, value: T }`, which receives any
///   other tag in `method`, with the remaining data in `value`.
#[proc_macro_derive(DeserializeTaggedEnum, attributes(tagged_enum))]
pub fn derive_deserialize_tagged_enum(item: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(item as syn::DeriveInput);
	tagged_enum::impl_deserialize(input).unwrap_or_else(|e| e.write_errors()).into()
}
