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

use alloc::vec::Vec;
use core::cmp::max;
use fp_evm::LinearCostPrecompile;
use evm::{ExitSucceed, ExitError};
use num::{BigUint, Zero, One};

pub struct Modexp;

impl LinearCostPrecompile for Modexp {
	const BASE: usize = 15;
	const WORD: usize = 3;

	fn execute(
		input: &[u8],
		_: usize,
	) -> core::result::Result<(ExitSucceed, Vec<u8>), ExitError> {
		let mut buf = [0; 32];
		buf.copy_from_slice(&input[0..32]);
		let mut len_bytes = [0u8; 8];
		len_bytes.copy_from_slice(&buf[24..]);
		let base_len = u64::from_be_bytes(len_bytes) as usize;

		buf = [0; 32];
		buf.copy_from_slice(&input[32..64]);
		len_bytes = [0u8; 8];
		len_bytes.copy_from_slice(&buf[24..]);
		let exp_len = u64::from_be_bytes(len_bytes) as usize;

		buf = [0; 32];
		buf.copy_from_slice(&input[64..96]);
		len_bytes = [0u8; 8];
		len_bytes.copy_from_slice(&buf[24..]);
		let mod_len = u64::from_be_bytes(len_bytes) as usize;

		// Gas formula allows arbitrary large exp_len when base and modulus are empty, so we need to handle empty base first.
		let r = if base_len == 0 && mod_len == 0 {
			BigUint::zero()
		} else {
			// read the numbers themselves.
			let mut buf = Vec::with_capacity(max(mod_len, max(base_len, exp_len)));
			buf.copy_from_slice(&input[0..base_len]);
			let base = BigUint::from_bytes_be(&buf[..base_len]);

			buf = Vec::with_capacity(max(mod_len, max(base_len, exp_len)));
			buf.copy_from_slice(&input[base_len..base_len + exp_len]);
			let exponent = BigUint::from_bytes_be(&buf[..exp_len]);

			buf = Vec::with_capacity(max(mod_len, max(base_len, exp_len)));
			buf.copy_from_slice(&input[(base_len + exp_len)..(base_len + exp_len + mod_len)]);
			let modulus = BigUint::from_bytes_be(&buf[..mod_len]);

			if modulus.is_zero() || modulus.is_one() {
				BigUint::zero()
			} else {
				base.modpow(&exponent, &modulus)
			}
		};

		// write output to given memory, left padded and same length as the modulus.
		let bytes = r.to_bytes_be();

		// always true except in the case of zero-length modulus, which leads to
		// output of length and value 1.
		if bytes.len() <= mod_len {
			let res_start = mod_len - bytes.len();
			let mut ret = Vec::with_capacity(bytes.len() - mod_len);
			ret.copy_from_slice(&bytes[res_start..bytes.len()]);
			Ok((ExitSucceed::Returned, ret.to_vec()))
		} else {
			Err(ExitError::Other("failed".into()))
		}
	}
}
