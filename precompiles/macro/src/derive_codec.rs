// This file is part of Tokfin.

// Copyright (c) Moonsong Labs.
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::{
	parse_macro_input, punctuated::Punctuated, spanned::Spanned, DeriveInput, Ident, LitStr, Path,
	PathSegment, PredicateType, TraitBound, TraitBoundModifier,
};

pub fn main(input: TokenStream) -> TokenStream {
	let DeriveInput {
		ident,
		mut generics,
		data,
		..
	} = parse_macro_input!(input as DeriveInput);

	let syn::Data::Struct(syn::DataStruct {
		fields: syn::Fields::Named(fields),
		..
	}) = data
	else {
		return quote_spanned! { ident.span() =>
			compile_error!("Codec can only be derived for structs with named fields");
		}
		.into();
	};
	let fields = fields.named;

	if fields.is_empty() {
		return quote_spanned! { ident.span() =>
			compile_error!("Codec can only be derived for structs with at least one field");
		}
		.into();
	}

	if let Some(unamed_field) = fields.iter().find(|f| f.ident.is_none()) {
		return quote_spanned! { unamed_field.ty.span() =>
			compile_error!("Codec can only be derived for structs with named fields");
		}
		.into();
	}

	let fields_ty: Vec<_> = fields.iter().map(|f| &f.ty).collect();
	let fields_ident: Vec<_> = fields
		.iter()
		.map(|f| f.ident.as_ref().expect("None case checked above"))
		.collect();
	let fields_name_lit: Vec<_> = fields_ident
		.iter()
		.map(|i| LitStr::new(&i.to_string(), i.span()))
		.collect();

	let evm_data_trait_path = {
		let mut segments = Punctuated::<PathSegment, _>::new();
		segments.push(Ident::new("precompile_utils", Span::call_site()).into());
		segments.push(Ident::new("solidity", Span::call_site()).into());
		segments.push(Ident::new("Codec", Span::call_site()).into());
		Path {
			leading_colon: Some(Default::default()),
			segments,
		}
	};
	let where_clause = generics.make_where_clause();

	for ty in &fields_ty {
		let mut bounds = Punctuated::new();
		bounds.push(
			TraitBound {
				paren_token: None,
				modifier: TraitBoundModifier::None,
				lifetimes: None,
				path: evm_data_trait_path.clone(),
			}
			.into(),
		);

		where_clause.predicates.push(
			PredicateType {
				lifetimes: None,
				bounded_ty: (*ty).clone(),
				colon_token: Default::default(),
				bounds,
			}
			.into(),
		);
	}

	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
	quote! {
		impl #impl_generics ::precompile_utils::solidity::codec::Codec for #ident #ty_generics
		#where_clause {
			fn read(
				reader: &mut ::precompile_utils::solidity::codec::Reader
			) -> ::precompile_utils::solidity::revert::MayRevert<Self> {
				use ::precompile_utils::solidity::revert::BacktraceExt as _;
				let (#(#fields_ident,)*): (#(#fields_ty,)*) = reader
					.read()
					.map_in_tuple_to_field(&[#(#fields_name_lit),*])?;
				Ok(Self {
					#(#fields_ident,)*
				})
			}

			fn write(writer: &mut ::precompile_utils::solidity::codec::Writer, value: Self) {
				::precompile_utils::solidity::codec::Codec::write(writer, (#(value.#fields_ident,)*));
			}

			fn has_static_size() -> bool {
				<(#(#fields_ty,)*)>::has_static_size()
			}

			fn signature() -> String {
				<(#(#fields_ty,)*)>::signature()
			}
		}
	}
	.into()
}
