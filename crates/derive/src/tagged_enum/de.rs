use darling::{FromDeriveInput, ast};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;

use crate::tagged_enum::types::{EnumVariant, EnumVariantField, TaggedEnum, TaggedEnumInput};

/// Upper bound for the capacity reserved up front for a flattened field, so that a bogus size hint
/// can't make us allocate a huge vector before reading any element.
const MAX_PREALLOCATED_ELEMENTS: usize = 4096;

pub(crate) fn impl_deserialize(input: syn::DeriveInput) -> darling::Result<TokenStream> {
	let tagged_enum = TaggedEnumInput::from_derive_input(&input)?;
	let tagged_enum = tagged_enum.validate()?;
	Ok(tagged_enum.impl_deserialize())
}

// All names the generated code introduces start with `__`, and every path is fully qualified, so
// that types, aliases (e.g. a local `Result`) or generic names in the user's module can't clash.

impl TaggedEnum {
	fn impl_deserialize(self) -> TokenStream {
		let ident = self.ident;
		let visitor = format_ident!("__{}Visitor", ident);

		let (known, other): (Vec<_>, Vec<_>) = self.variants.into_iter().partition(|variant| !variant.is_other());
		let other = other
			.into_iter()
			.next()
			.expect("validated enum must contain an `Other` variant");

		let tag = gen_tag(&known);
		let ignore_trailing = known.iter().any(EnumVariant::has_flattened_field).then(gen_ignore_trailing);

		let known_arms = known.into_iter().enumerate().map(|(index, variant)| {
			let tag = tag_variant(index);
			let body = variant.gen_de_ctor(&ident, false);
			quote! { __Tag::#tag => { #body } }
		});
		let other_body = other.gen_de_ctor(&ident, true);

		quote! {
			impl<'de> ::serde::Deserialize<'de> for #ident {
				fn deserialize<__D>(__deserializer: __D) -> ::core::result::Result<Self, __D::Error>
				where
					__D: ::serde::Deserializer<'de>,
				{
					#tag
					#ignore_trailing

					struct #visitor;

					impl<'de> ::serde::de::Visitor<'de> for #visitor {
						type Value = #ident;

						fn expecting(&self, __formatter: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
							__formatter.write_str("a tagged enum sequence")
						}

						fn visit_seq<__A>(self, mut __seq: __A) -> ::core::result::Result<Self::Value, __A::Error>
						where
							__A: ::serde::de::SeqAccess<'de>,
						{
							let __tag = __seq
								.next_element::<__Tag>()?
								.ok_or_else(|| ::serde::de::Error::invalid_length(0, &self))?;

							match __tag {
								#(#known_arms)*
								__Tag::__Other(__method) => { #other_body }
							}
						}
					}

					__deserializer.deserialize_seq(#visitor)
				}
			}
		}
	}
}

fn tag_variant(index: usize) -> syn::Ident {
	format_ident!("__Variant{}", index)
}

/// Generates `__Tag`, which reads the tag (the first element) and identifies the variant. Known tags are
/// matched without allocating; only unknown ones are kept as a `String` for the fallback variant.
///
/// It accepts exactly what `String` accepts, with the same error messages: msgpack `str`, and `bin` that
/// holds UTF-8.
fn gen_tag(known: &[EnumVariant]) -> TokenStream {
	let tags = (0..known.len()).map(tag_variant).collect::<Vec<_>>();
	let names = known.iter().map(EnumVariant::tag_name).collect::<Vec<_>>();

	quote! {
		enum __Tag {
			#(#tags,)*
			__Other(::std::string::String),
		}

		impl<'de> ::serde::Deserialize<'de> for __Tag {
			fn deserialize<__D>(__deserializer: __D) -> ::core::result::Result<Self, __D::Error>
			where
				__D: ::serde::Deserializer<'de>,
			{
				struct __TagVisitor;

				impl __TagVisitor {
					fn known(__name: &str) -> ::core::option::Option<__Tag> {
						match __name {
							#(#names => ::core::option::Option::Some(__Tag::#tags),)*
							_ => ::core::option::Option::None,
						}
					}
				}

				impl<'de> ::serde::de::Visitor<'de> for __TagVisitor {
					type Value = __Tag;

					fn expecting(&self, __formatter: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
						__formatter.write_str("a string")
					}

					fn visit_str<__E>(self, __value: &str) -> ::core::result::Result<Self::Value, __E>
					where
						__E: ::serde::de::Error,
					{
						::core::result::Result::Ok(
							Self::known(__value).unwrap_or_else(|| __Tag::__Other(__value.to_owned())),
						)
					}

					fn visit_string<__E>(
						self,
						__value: ::std::string::String,
					) -> ::core::result::Result<Self::Value, __E>
					where
						__E: ::serde::de::Error,
					{
						::core::result::Result::Ok(Self::known(&__value).unwrap_or(__Tag::__Other(__value)))
					}

					fn visit_bytes<__E>(self, __value: &[u8]) -> ::core::result::Result<Self::Value, __E>
					where
						__E: ::serde::de::Error,
					{
						match ::core::str::from_utf8(__value) {
							::core::result::Result::Ok(__value) => self.visit_str(__value),
							::core::result::Result::Err(_) => ::core::result::Result::Err(
								::serde::de::Error::invalid_value(::serde::de::Unexpected::Bytes(__value), &self),
							),
						}
					}

					fn visit_byte_buf<__E>(
						self,
						__value: ::std::vec::Vec<u8>,
					) -> ::core::result::Result<Self::Value, __E>
					where
						__E: ::serde::de::Error,
					{
						match ::std::string::String::from_utf8(__value) {
							::core::result::Result::Ok(__value) => self.visit_string(__value),
							::core::result::Result::Err(__error) => {
								::core::result::Result::Err(::serde::de::Error::invalid_value(
									::serde::de::Unexpected::Bytes(&__error.into_bytes()),
									&self,
								))
							},
						}
					}
				}

				__deserializer.deserialize_str(__TagVisitor)
			}
		}
	}
}

/// Generates `__IgnoreTrailing<__T>`, which deserializes `__T` from the start of a sequence and skips any
/// elements after the ones `__T` reads. Neovim may append parameters to existing events, and rmp_serde
/// rejects a sequence that isn't read to the end.
fn gen_ignore_trailing() -> TokenStream {
	quote! {
		struct __IgnoreTrailing<__T>(__T);

		impl<'de, __T> ::serde::Deserialize<'de> for __IgnoreTrailing<__T>
		where
			__T: ::serde::Deserialize<'de>,
		{
			fn deserialize<__D>(__deserializer: __D) -> ::core::result::Result<Self, __D::Error>
			where
				__D: ::serde::Deserializer<'de>,
			{
				struct __IgnoreTrailingVisitor<__T>(::core::marker::PhantomData<__T>);

				impl<'de, __T> ::serde::de::Visitor<'de> for __IgnoreTrailingVisitor<__T>
				where
					__T: ::serde::Deserialize<'de>,
				{
					type Value = __IgnoreTrailing<__T>;

					fn expecting(&self, __formatter: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
						__formatter.write_str("a sequence of arguments")
					}

					fn visit_seq<__A>(self, mut __seq: __A) -> ::core::result::Result<Self::Value, __A::Error>
					where
						__A: ::serde::de::SeqAccess<'de>,
					{
						let __value = __T::deserialize(::serde::de::value::SeqAccessDeserializer::new(&mut __seq))?;
						while __seq.next_element::<::serde::de::IgnoredAny>()?.is_some() {}
						::core::result::Result::Ok(__IgnoreTrailing(__value))
					}
				}

				__deserializer.deserialize_seq(__IgnoreTrailingVisitor(::core::marker::PhantomData))
			}
		}
	}
}

impl EnumVariant {
	fn has_flattened_field(&self) -> bool {
		self.fields.fields.iter().any(|field| field.flatten.is_some())
	}

	/// Generates the code that reads the rest of the sequence into this variant. For the fallback
	/// variant, `__method` holds the tag.
	fn gen_de_ctor(self, enum_ident: &syn::Ident, is_fallback: bool) -> TokenStream {
		let variant_name = self.ident;
		let fields = self.fields.fields;
		let vars = (0..fields.len())
			.map(|index| format_ident!("__field_{}", index))
			.collect::<Vec<_>>();

		let construct = match self.fields.style {
			ast::Style::Tuple => quote! { #enum_ident::#variant_name(#(#vars),*) },
			ast::Style::Struct => {
				let names = fields
					.iter()
					.map(|field| field.ident.as_ref().expect("struct fields are named"));
				quote! { #enum_ident::#variant_name { #(#names: #vars),* } }
			},
			// The event has no parameters: skip whatever follows the tag (`["flush", []]` carries one empty
			// argument list).
			ast::Style::Unit => {
				return quote! {
					while __seq.next_element::<::serde::de::IgnoredAny>()?.is_some() {}
					::core::result::Result::Ok(#enum_ident::#variant_name)
				};
			},
		};

		// The tag is element 0. `Other.method` is filled from the tag, so it reads no element.
		let mut element = 0;
		let reads = fields
			.into_iter()
			.enumerate()
			.map(|(index, field)| {
				if !(is_fallback && field.is_method()) {
					element += 1;
				}
				field.gen_de(index, element, is_fallback)
			})
			.collect::<Vec<_>>();

		quote! {
			#(#reads)*
			::core::result::Result::Ok(#construct)
		}
	}
}

impl EnumVariantField {
	/// Generates the code that reads this field, the `index`-th of its variant, into `__field_{index}`.
	/// `element` is its position in the sequence, used in error messages.
	fn gen_de(self, index: usize, element: usize, is_fallback: bool) -> TokenStream {
		let var = format_ident!("__field_{}", index);

		if is_fallback && self.is_method() {
			return quote! { let #var = __method; };
		}

		// A flattened field takes all remaining elements. In known variants each element is an argument
		// list, which may have trailing arguments we don't know; the fallback keeps elements as they are,
		// since they aren't necessarily sequences.
		if let Some(element_ty) = self.flatten {
			// Spanned at the element type, so a type that can't be deserialized is reported there.
			let span = element_ty.span();
			let next_element = if is_fallback {
				let next = quote_spanned! {span=> next_element::<#element_ty> };
				quote! { __seq.#next()? }
			} else {
				let next = quote_spanned! {span=> next_element::<__IgnoreTrailing<#element_ty>> };
				quote! { __seq.#next()?.map(|__IgnoreTrailing(__value)| __value) }
			};

			return quote! {
				let mut #var = ::std::vec::Vec::with_capacity(::core::cmp::min(
					__seq.size_hint().unwrap_or(0),
					#MAX_PREALLOCATED_ELEMENTS,
				));
				while let ::core::option::Option::Some(__value) = #next_element {
					#var.push(__value);
				}
			};
		}

		let ty = self.ty;
		// Spanned at the field type, so a type that can't be deserialized is reported there.
		let next = quote_spanned! {ty.span()=> next_element::<#ty> };

		quote! {
			let #var = __seq
				.#next()?
				.ok_or_else(|| ::serde::de::Error::invalid_length(#element, &self))?;
		}
	}
}
