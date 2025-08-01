// This file is part of Frontier.

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

use proc_macro2::Span;
use quote::ToTokens;
use syn::spanned::Spanned;

pub fn take_attributes<A>(attributes: &mut Vec<syn::Attribute>) -> syn::Result<Vec<A>>
where
	A: syn::parse::Parse,
{
	let mut output = vec![];
	let pred = |attr: &syn::Attribute| {
		attr.path()
			.segments
			.first()
			.is_some_and(|segment| segment.ident == "precompile")
	};

	while let Some(index) = attributes.iter().position(pred) {
		let attr = attributes.remove(index);
		let attr = syn::parse2(attr.into_token_stream())?;
		output.push(attr)
	}
	Ok(output)
}

/// List of additional token to be used for parsing.
pub mod keyword {
	syn::custom_keyword!(precompile);
	syn::custom_keyword!(public);
	syn::custom_keyword!(fallback);
	syn::custom_keyword!(payable);
	syn::custom_keyword!(view);
	syn::custom_keyword!(discriminant);
	syn::custom_keyword!(precompile_set);
	syn::custom_keyword!(test_concrete_types);
	syn::custom_keyword!(pre_check);
}

/// Attributes for methods
#[allow(dead_code)]
pub enum MethodAttr {
	Public(Span, syn::LitStr),
	Fallback(Span),
	Payable(Span),
	View(Span),
	Discriminant(Span),
	PreCheck(Span),
}

impl syn::parse::Parse for MethodAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<syn::Token![#]>()?;
		let content;
		syn::bracketed!(content in input);
		content.parse::<keyword::precompile>()?;
		content.parse::<syn::Token![::]>()?;

		let lookahead = content.lookahead1();

		if lookahead.peek(keyword::public) {
			let span = content.parse::<keyword::public>()?.span();

			let inner;
			syn::parenthesized!(inner in content);
			let signature = inner.parse::<syn::LitStr>()?;

			Ok(MethodAttr::Public(span, signature))
		} else if lookahead.peek(keyword::fallback) {
			Ok(MethodAttr::Fallback(
				content.parse::<keyword::fallback>()?.span(),
			))
		} else if lookahead.peek(keyword::payable) {
			Ok(MethodAttr::Payable(
				content.parse::<keyword::payable>()?.span(),
			))
		} else if lookahead.peek(keyword::view) {
			Ok(MethodAttr::View(content.parse::<keyword::view>()?.span()))
		} else if lookahead.peek(keyword::discriminant) {
			Ok(MethodAttr::Discriminant(
				content.parse::<keyword::discriminant>()?.span(),
			))
		} else if lookahead.peek(keyword::pre_check) {
			Ok(MethodAttr::PreCheck(
				content.parse::<keyword::pre_check>()?.span(),
			))
		} else {
			Err(lookahead.error())
		}
	}
}

/// Attributes for the main impl Block.
#[allow(dead_code)]
pub enum ImplAttr {
	PrecompileSet(Span),
	TestConcreteTypes(Span, Vec<syn::Type>),
}

impl syn::parse::Parse for ImplAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		input.parse::<syn::Token![#]>()?;
		let content;
		syn::bracketed!(content in input);
		content.parse::<keyword::precompile>()?;
		content.parse::<syn::Token![::]>()?;

		let lookahead = content.lookahead1();

		if lookahead.peek(keyword::precompile_set) {
			Ok(ImplAttr::PrecompileSet(
				content.parse::<keyword::precompile_set>()?.span(),
			))
		} else if lookahead.peek(keyword::test_concrete_types) {
			let span = content.parse::<keyword::test_concrete_types>()?.span();

			let inner;
			syn::parenthesized!(inner in content);
			let types = inner.parse_terminated(syn::Type::parse, syn::Token![,])?;

			Ok(ImplAttr::TestConcreteTypes(
				span,
				types.into_iter().collect(),
			))
		} else {
			Err(lookahead.error())
		}
	}
}
