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

use sp_crypto_hashing::keccak_256;

#[test]
fn test_keccak256() {
	assert_eq!(
		&precompile_utils_macro::keccak256!(""),
		keccak_256(b"").as_slice(),
	);
	assert_eq!(
		&precompile_utils_macro::keccak256!("toto()"),
		keccak_256(b"toto()").as_slice(),
	);
	assert_ne!(
		&precompile_utils_macro::keccak256!("toto()"),
		keccak_256(b"tata()").as_slice(),
	);
}

#[test]
#[ignore]
fn ui() {
	let t = trybuild::TestCases::new();
	t.compile_fail("tests/compile-fail/**/*.rs");
	t.pass("tests/pass/**/*.rs");
}

// Cargo expand is not supported on stable rust
#[test]
#[ignore]
fn expand() {
	// Use `expand` to update the expansions
	// Replace it with `expand_without_refresh` afterward so that
	// CI checks the extension don't change

	// macrotest::expand("tests/expand/**/*.rs");
	macrotest::expand_without_refresh("tests/expand/**/*.rs");
}
