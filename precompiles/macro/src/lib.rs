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

#![crate_type = "proc-macro"]
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use sp_crypto_hashing::keccak_256;
use syn::{parse_macro_input, spanned::Spanned, Expr, Ident, ItemType, Lit, LitStr};

mod derive_codec;
mod precompile;
mod precompile_name_from_address;

struct Bytes(Vec<u8>);

impl ::std::fmt::Debug for Bytes {
	#[inline]
	fn fmt(&self, f: &mut std::fmt::Formatter) -> ::std::fmt::Result {
		let data = &self.0;
		write!(f, "[")?;
		if !data.is_empty() {
			write!(f, "{:#04x}u8", data[0])?;
			for unit in data.iter().skip(1) {
				write!(f, ", {:#04x}", unit)?;
			}
		}
		write!(f, "]")
	}
}

#[proc_macro]
pub fn keccak256(input: TokenStream) -> TokenStream {
	let lit_str = parse_macro_input!(input as LitStr);

	let hash = keccak_256(lit_str.value().as_bytes());

	let bytes = Bytes(hash.to_vec());
	let eval_str = format!("{:?}", bytes);
	let eval_ts: proc_macro2::TokenStream = eval_str.parse().unwrap_or_else(|_| {
		panic!(
			"Failed to parse the string \"{}\" to TokenStream.",
			eval_str
		);
	});
	quote!(#eval_ts).into()
}

#[proc_macro_attribute]
pub fn precompile(attr: TokenStream, input: TokenStream) -> TokenStream {
	precompile::main(attr, input)
}

#[proc_macro_attribute]
pub fn precompile_name_from_address(attr: TokenStream, input: TokenStream) -> TokenStream {
	precompile_name_from_address::main(attr, input)
}

#[proc_macro_derive(Codec)]
pub fn derive_codec(input: TokenStream) -> TokenStream {
	derive_codec::main(input)
}
