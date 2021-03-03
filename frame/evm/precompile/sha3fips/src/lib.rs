// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
//
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use tiny_keccak::Hasher;
use alloc::vec::Vec;

use fp_evm::LinearCostPrecompile;
use evm::{ExitSucceed, ExitError};

pub struct Blake2F;

impl LinearCostPrecompile for Blake2F {
	const BASE: u64 = 60;
	const WORD: u64 = 12;

	fn execute(
		input: &[u8],
		_: u64,
	) -> core::result::Result<(ExitSucceed, Vec<u8>), ExitError> {
		let mut output = [0; 32];
		let mut sha3 = tiny_keccak::Sha3::v256();
		sha3.update(input);
		sha3.finalize(&mut output);
		Ok((ExitSucceed::Returned, output.to_vec()))
	}
}
