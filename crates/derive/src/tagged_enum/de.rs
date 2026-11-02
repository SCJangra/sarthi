use darling::{FromDeriveInput, ast};
use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::tagged_enum::types::{EnumVariant, EnumVariantField, TaggedEnum, TaggedEnumInput};

pub(crate) fn impl_deserialize(input: syn::DeriveInput) -> darling::Result<TokenStream> {
	let tagged_enum = TaggedEnumInput::from_derive_input(&input)?;
	let tagged_enum = tagged_enum.validate()?;
	Ok(tagged_enum.impl_deserialize())
}

impl TaggedEnum {
	fn impl_deserialize(self) -> TokenStream {
		let ident = self.ident;
		let visitor = format_ident!("{}Visitor", ident);

		let mut variant_arms = Vec::new();
		let mut fallback_arm = None;

		let has_flattened_known_fields = self.variants.iter().any(|variant| {
			variant.ident != "Other" && variant.fields.fields.iter().any(|field| field.flatten.is_some())
		});
		let ignore_trailing = has_flattened_known_fields.then(gen_ignore_trailing);

		for variant in self.variants {
			if variant.ident == "Other" {
				fallback_arm = Some(variant.gen_unknown_de_arm(&ident));
			} else {
				variant_arms.push(variant.gen_known_de_arm(&ident));
			}
		}

		let fallback_arm = fallback_arm.expect("validated enum must contain an `Other` variant");

		quote! {
			impl<'de> ::serde::Deserialize<'de> for #ident {
				fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
				where
					D: ::serde::Deserializer<'de>,
				{
					#ignore_trailing

					struct #visitor;

					impl<'de> ::serde::de::Visitor<'de> for #visitor {
						type Value = #ident;

						fn expecting(
							&self,
							formatter: &mut ::std::fmt::Formatter,
						) -> ::std::fmt::Result {
							formatter.write_str("a tagged enum sequence")
						}

						fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
						where
							A: ::serde::de::SeqAccess<'de>,
						{
							let method = seq
								.next_element::<::std::string::String>()?
								.ok_or_else(|| ::serde::de::Error::invalid_length(0, &self))?;

							match method.as_str() {
								#(#variant_arms)*
								#fallback_arm
							}
						}
					}

					deserializer.deserialize_seq(#visitor)
				}
			}
		}
	}
}

/// Generates `IgnoreTrailing<T>`, which deserializes `T` from the start of a sequence and skips any
/// elements after the ones `T` reads. Neovim adds trailing parameters to events over time, and
/// rmp_serde rejects a sequence that isn't read to the end.
fn gen_ignore_trailing() -> TokenStream {
	quote! {
		struct IgnoreTrailing<T>(T);

		impl<'de, T> ::serde::Deserialize<'de> for IgnoreTrailing<T>
		where
			T: ::serde::Deserialize<'de>,
		{
			fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
			where
				D: ::serde::Deserializer<'de>,
			{
				struct IgnoreTrailingVisitor<T>(::std::marker::PhantomData<T>);

				impl<'de, T> ::serde::de::Visitor<'de> for IgnoreTrailingVisitor<T>
				where
					T: ::serde::Deserialize<'de>,
				{
					type Value = IgnoreTrailing<T>;

					fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
						formatter.write_str("a sequence of arguments")
					}

					fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
					where
						A: ::serde::de::SeqAccess<'de>,
					{
						let value = T::deserialize(::serde::de::value::SeqAccessDeserializer::new(&mut seq))?;
						while seq.next_element::<::serde::de::IgnoredAny>()?.is_some() {}
						Ok(IgnoreTrailing(value))
					}
				}

				deserializer.deserialize_seq(IgnoreTrailingVisitor(::std::marker::PhantomData))
			}
		}
	}
}

impl EnumVariant {
	fn gen_known_de_arm(self, enum_ident: &syn::Ident) -> TokenStream {
		let name = self.ident.to_string().to_snake_case();
		let arm_body = self.gen_de_ctor(enum_ident, false);

		quote! {
			#name => {
				#arm_body
			},
		}
	}

	fn gen_unknown_de_arm(self, enum_ident: &syn::Ident) -> TokenStream {
		let arm_body = self.gen_de_ctor(enum_ident, true);

		quote! {
			_ => {
				#arm_body
			},
		}
	}

	fn gen_de_ctor(self, enum_ident: &syn::Ident, inject_method_from_tag: bool) -> TokenStream {
		let variant_name = self.ident;

		match self.fields.style {
			ast::Style::Tuple => {
				let fields = self.fields.fields;
				let de = fields
					.into_iter()
					.enumerate()
					.map(|(idx, field)| field.gen_de(idx + 1, inject_method_from_tag))
					.collect::<Vec<_>>();

				let values =
					(0..de.len()).map(|idx| format_ident!("field_{}", idx)).collect::<Vec<_>>();

				quote! {
					#(#de)*
					Ok(#enum_ident::#variant_name(#(#values),*))
				}
			},
			ast::Style::Struct => {
				let fields = self.fields.fields;
				let field_idents = fields
					.iter()
					.map(|field| field.ident.as_ref().expect("struct fields must be named").clone())
					.collect::<Vec<_>>();

				let de = fields
					.into_iter()
					.enumerate()
					.map(|(idx, field)| field.gen_de(idx + 1, inject_method_from_tag))
					.collect::<Vec<_>>();

				let values = field_idents
					.into_iter()
					.enumerate()
					.map(|(idx, key)| {
						let value = format_ident!("field_{}", idx);
						quote! { #key: #value }
					})
					.collect::<Vec<_>>();

				quote! {
					#(#de)*
					Ok(#enum_ident::#variant_name { #(#values),* })
				}
			},
			ast::Style::Unit => quote! { Ok(#enum_ident::#variant_name) },
		}
	}
}

impl EnumVariantField {
	fn gen_de(self, index: usize, inject_method_from_tag: bool) -> TokenStream {
		let var = format_ident!("field_{}", index - 1);

		if inject_method_from_tag
			&& let Some(field_ident) = &self.ident
			&& field_ident == "method"
		{
			return quote! {
				let #var = method.clone();
			};
		}

		// Known variants tolerate trailing arguments in each element. `Other` keeps its elements as
		// they are, since they aren't necessarily sequences.
		if let Some(element_ty) = self.flatten {
			let next_element = if inject_method_from_tag {
				quote! { seq.next_element::<#element_ty>()? }
			} else {
				quote! { seq.next_element::<IgnoreTrailing<#element_ty>>()?.map(|IgnoreTrailing(value)| value) }
			};

			return quote! {
				let mut #var = ::std::vec::Vec::new();
				while let Some(value) = #next_element {
					#var.push(value);
				}
			};
		}

		let ty = self.ty;

		quote! {
			let #var = seq
				.next_element::<#ty>()?
				.ok_or_else(|| ::serde::de::Error::invalid_length(#index, &self))?;
		}
	}
}
