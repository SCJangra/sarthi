use std::collections::HashMap;

use darling::util::Flag;
use darling::{FromDeriveInput, FromField, FromVariant, ast};
use heck::ToSnakeCase;
use syn::{Ident, Type};

use crate::utils;

/// Name of the fallback variant, which receives every tag no other variant matches
const OTHER: &str = "Other";

#[derive(FromDeriveInput)]
#[darling(attributes(tagged_enum))]
pub(crate) struct TaggedEnumInput {
	ident: Ident,
	generics: syn::Generics,
	data: ast::Data<EnumVariantInput, ()>,

	/// Only accepted to report that it belongs on a field
	#[darling(default)]
	flatten: Flag,
}

pub(crate) struct TaggedEnum {
	pub(crate) ident: Ident,
	pub(crate) variants: Vec<EnumVariant>,
}

#[derive(FromVariant)]
#[darling(attributes(tagged_enum))]
pub(crate) struct EnumVariantInput {
	pub(crate) ident: Ident,
	pub(crate) fields: ast::Fields<EnumVariantFieldInput>,

	/// Only accepted to report that it belongs on a field
	#[darling(default)]
	pub(crate) flatten: Flag,
}

pub(crate) struct EnumVariant {
	pub(crate) ident: Ident,
	pub(crate) fields: ast::Fields<EnumVariantField>,
}

#[derive(FromField)]
#[darling(attributes(tagged_enum))]
pub(crate) struct EnumVariantFieldInput {
	pub(crate) ident: Option<Ident>,
	pub(crate) ty: Type,

	#[darling(default)]
	pub(crate) flatten: bool,
}

pub(crate) struct EnumVariantField {
	pub(crate) ident: Option<Ident>,
	pub(crate) ty: Type,

	/// For `#[tagged_enum(flatten)]` fields, the element type `T` of the field's `Vec<T>`
	pub(crate) flatten: Option<Type>,
}

impl TryFrom<EnumVariantInput> for EnumVariant {
	type Error = darling::Error;

	fn try_from(value: EnumVariantInput) -> Result<Self, Self::Error> {
		let mut errors = darling::Error::accumulator();

		if value.flatten.is_present() {
			let msg = "`#[tagged_enum(flatten)]` goes on a field, not on a variant";
			errors.push(darling::Error::custom(msg).with_span(&value.flatten.span()));
		}

		let style = value.fields.style;
		let fields = value
			.fields
			.into_iter()
			.filter_map(|field| errors.handle(EnumVariantField::try_from(field)))
			.collect::<Vec<_>>();

		let fields = ast::Fields::new(style, fields);

		errors.finish_with(Self { ident: value.ident, fields })
	}
}

impl TryFrom<EnumVariantFieldInput> for EnumVariantField {
	type Error = darling::Error;

	fn try_from(value: EnumVariantFieldInput) -> Result<Self, Self::Error> {
		let flatten = value.flatten.then(|| utils::vec_inner_type(&value.ty)).flatten();

		if value.flatten && flatten.is_none() {
			let msg = "`#[tagged_enum(flatten)]` field must be a `Vec<T>`, since it collects the remaining elements";
			let error = darling::Error::custom(msg).with_span(&value.ty);
			return Err(error);
		}

		Ok(Self { ident: value.ident, ty: value.ty, flatten })
	}
}

impl TaggedEnumInput {
	pub(crate) fn validate(self) -> darling::Result<TaggedEnum> {
		let Some(variants) = self.data.take_enum() else {
			let msg = "`DeserializeTaggedEnum` can only be derived for enums";
			return Err(darling::Error::custom(msg).with_span(&self.ident));
		};

		let mut errors = darling::Error::accumulator();

		if self.flatten.is_present() {
			let msg = "`#[tagged_enum(flatten)]` goes on a field, not on the enum";
			errors.push(darling::Error::custom(msg).with_span(&self.flatten.span()));
		}

		if !self.generics.params.is_empty() {
			let msg = "`DeserializeTaggedEnum` doesn't support generic enums";
			errors.push(darling::Error::custom(msg).with_span(&self.generics));
		}

		// Decided from the names alone, so an `Other` with invalid fields reports those, not a missing `Other`.
		let has_other = variants.iter().any(|variant| variant.ident == OTHER);

		let variants = variants
			.into_iter()
			.filter_map(|variant| errors.handle(EnumVariant::try_from(variant)))
			.collect::<Vec<_>>();

		let mut tags: HashMap<String, &Ident> = HashMap::new();

		for variant in &variants {
			if variant.is_other() {
				if errors.handle(variant.validate_other()).is_some() {
					errors.handle(variant.validate_flatten_last());
				}
				continue;
			}

			errors.handle(variant.validate_flatten_last());

			// Two variants with the same tag: the second one could never be selected.
			let tag = variant.tag_name();
			if let Some(first) = tags.insert(tag.clone(), &variant.ident) {
				let msg = format!(
					"`{}` has the same tag as `{first}` (`{tag}`), so it could never be selected; rename one of them",
					variant.ident
				);
				errors.push(darling::Error::custom(msg).with_span(&variant.ident));
			}
		}

		if !has_other {
			let msg = "tagged enum must have a fallback variant `Other { method: String, value: T }`, which receives unknown tags";
			errors.push(darling::Error::custom(msg).with_span(&self.ident));
		}

		errors.finish_with(TaggedEnum { ident: self.ident, variants })
	}
}

impl EnumVariant {
	/// Whether this is the fallback variant
	pub(crate) fn is_other(&self) -> bool {
		self.ident == OTHER
	}

	/// The tag that selects this variant: its name in snake case, e.g. `GridLine` → `grid_line`
	pub(crate) fn tag_name(&self) -> String {
		self.ident.to_string().to_snake_case()
	}

	/// A flattened field takes all remaining elements, so no field that reads an element can follow it.
	/// (`Other.method` is filled from the tag and reads no element.)
	fn validate_flatten_last(&self) -> darling::Result<()> {
		let mut reading = self
			.fields
			.fields
			.iter()
			.filter(|field| !(self.is_other() && field.is_method()));

		if let Some(flattened) = reading.by_ref().find(|field| field.flatten.is_some())
			&& reading.next().is_some()
		{
			let msg =
				"`#[tagged_enum(flatten)]` field must be the last field, since it collects the remaining elements";
			return Err(darling::Error::custom(msg).with_span(&flattened.ty));
		}

		Ok(())
	}

	fn validate_other(&self) -> darling::Result<()> {
		if self.fields.style != ast::Style::Struct {
			let msg = "`Other` must be a struct variant: `Other { method: String, value: T }`";
			return Err(darling::Error::custom(msg).with_span(&self.ident));
		}

		if self.fields.fields.len() != 2 {
			let msg = format!(
				"`Other` must have exactly two fields, `method: String` and `value`, but has {}",
				self.fields.fields.len()
			);
			return Err(darling::Error::custom(msg).with_span(&self.ident));
		}

		let method_field = self.fields.fields.iter().find(|field| field.is_method()).ok_or_else(|| {
			let msg = "`Other` must have a `method: String` field, which receives the tag";
			darling::Error::custom(msg).with_span(&self.ident)
		})?;

		if !utils::is_string_type(&method_field.ty) {
			let msg = "`Other.method` must have type `String`";
			return Err(darling::Error::custom(msg).with_span(&method_field.ty));
		}

		let has_value = self
			.fields
			.fields
			.iter()
			.any(|field| field.ident.as_ref().is_some_and(|ident| ident == "value"));

		if !has_value {
			let msg = "`Other` must have a `value` field, which receives the data after the tag";
			return Err(darling::Error::custom(msg).with_span(&self.ident));
		}

		Ok(())
	}
}

impl EnumVariantField {
	/// Whether this is the `method` field, which the fallback variant fills with the tag
	pub(crate) fn is_method(&self) -> bool {
		self.ident.as_ref().is_some_and(|ident| ident == "method")
	}
}
