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
use num::{BigUint, Zero, One, ToPrimitive, FromPrimitive};

pub struct Modexp;

// ModExp expects the following as inputs:
// 1) 32 bytes expressing the length of base
// 2) 32 bytes expressing the length of exponent
// 3) 32 bytes expressing the length of modulus
// 4) base, size as described above
// 5) exponent, size as described above
// 6) modulus, size as described above
//
//
// NOTE: input sizes are arbitrarily large (up to 256 bits), with the expectation
//       that gas limits would be applied before actual computation.
//
//       maximum stack size will also prevent abuse.
//
//       see: https://eips.ethereum.org/EIPS/eip-198

impl LinearCostPrecompile for Modexp {
	const BASE: usize = 15;
	const WORD: usize = 3;

	fn execute(
		input: &[u8],
		_: usize,
	) -> core::result::Result<(ExitSucceed, Vec<u8>), ExitError> {
		if input.len() < 96 {
			return Err(ExitError::Other("input must contain at least 96 bytes".into()));
		};

		// reasonable assumption: this must fit within the Ethereum EVM's max stack size
		let max_size_big = BigUint::from_u32(1024).expect("can't create BigUint");

		let mut buf = [0; 32];
		buf.copy_from_slice(&input[0..32]);
		let base_len_big = BigUint::from_bytes_be(&buf);
		if base_len_big > max_size_big {
			return Err(ExitError::Other("unreasonably large base length".into()));
		}

		buf.copy_from_slice(&input[32..64]);
		let exp_len_big = BigUint::from_bytes_be(&buf);
		if exp_len_big > max_size_big {
			return Err(ExitError::Other("unreasonably large exponent length".into()));
		}

		buf.copy_from_slice(&input[64..96]);
		let mod_len_big = BigUint::from_bytes_be(&buf);
		if mod_len_big > max_size_big {
			return Err(ExitError::Other("unreasonably large exponent length".into()));
		}

		// bounds check handled above
		let base_len = base_len_big.to_usize().expect("base_len out of bounds");
		let exp_len = exp_len_big.to_usize().expect("exp_len out of bounds");
		let mod_len = mod_len_big.to_usize().expect("mod_len out of bounds");

		// input length should be at least 96 + user-specified length of base + exp + mod
		let total_len = base_len + exp_len + mod_len + 96;
		if input.len() < total_len {
			let err_msg = format!("expected at least {} bytes but received {}", total_len, input.len());
			return Err(ExitError::Other(err_msg.into()));
		}

		// Gas formula allows arbitrary large exp_len when base and modulus are empty, so we need to handle empty base first.
		let r = if base_len == 0 && mod_len == 0 {
			BigUint::zero()
		} else {

			// read the numbers themselves.
			let base_start = 96; // previous 3 32-byte fields
			let base = BigUint::from_bytes_be(&input[base_start..base_start + base_len]);

			let exp_start = base_start + base_len;
			let exponent = BigUint::from_bytes_be(&input[exp_start..exp_start + exp_len]);

			let mod_start = exp_start + exp_len;
			let modulus = BigUint::from_bytes_be(&input[mod_start..mod_start + mod_len]);

			// TODO: computation should only proceed if sufficient gas has been provided

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
		if bytes.len() == mod_len {
			Ok((ExitSucceed::Returned, bytes.to_vec()))
		} else if bytes.len() < mod_len {
			let mut ret = Vec::with_capacity(mod_len);
			ret.extend(core::iter::repeat(0).take(mod_len - bytes.len()));
			ret.extend_from_slice(&bytes[..]);
			Ok((ExitSucceed::Returned, ret.to_vec()))
		} else {
			Err(ExitError::Other("failed".into()))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	extern crate hex;

	#[test]
	fn test_empty_input() -> std::result::Result<(), ExitError> {

		let input: [u8; 0] = [];
		let cost: usize = 1;

		match Modexp::execute(&input, cost) {
			Ok((_, _)) => {
				panic!("Test not expected to pass");
			},
			Err(e) => {
				assert_eq!(e, ExitError::Other("input must contain at least 96 bytes".into()));
				Ok(())
			}
		}
	}

	#[test]
	fn test_insufficient_input() -> std::result::Result<(), ExitError> {

		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001")
			.expect("Decode failed");

		let cost: usize = 1;

		match Modexp::execute(&input, cost) {
			Ok((_, _)) => {
				panic!("Test not expected to pass");
			},
			Err(e) => {
				assert_eq!(e, ExitError::Other("expected at least 99 bytes but received 96".into()));
				Ok(())
			}
		}
	}

	#[test]
	fn test_excessive_input() -> std::result::Result<(), ExitError> {

		let input = hex::decode(
			"1000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001")
			.expect("Decode failed");

		let cost: usize = 1;

		match Modexp::execute(&input, cost) {
			Ok((_, _)) => {
				panic!("Test not expected to pass");
			},
			Err(e) => {
				assert_eq!(e, ExitError::Other("unreasonably large base length".into()));
				Ok(())
			}
		}
	}

	#[test]
	fn test_simple_inputs() {

		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001\
			03\
			05\
			07").expect("Decode failed");

		// 3 ^ 5 % 7 == 5

		let cost: usize = 1;

		match Modexp::execute(&input, cost) {
			Ok((_, output)) => {
				assert_eq!(output.len(), 1); // should be same length as mod
				let result = BigUint::from_bytes_be(&output[..]);
				let expected = BigUint::parse_bytes(b"5", 10).unwrap();
				assert_eq!(result, expected);
			},
			Err(_) => {
				panic!("Modexp::execute() returned error"); // TODO: how to pass error on?
			}
		}
	}

	#[test]
	fn test_large_inputs() {

		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000020\
			0000000000000000000000000000000000000000000000000000000000000020\
			0000000000000000000000000000000000000000000000000000000000000020\
			000000000000000000000000000000000000000000000000000000000000EA5F\
			0000000000000000000000000000000000000000000000000000000000000015\
			0000000000000000000000000000000000000000000000000000000000003874")
			.expect("Decode failed");

		// 59999 ^ 21 % 14452 = 10055

		let cost: usize = 1;

		match Modexp::execute(&input, cost) {
			Ok((_, output)) => {
				assert_eq!(output.len(), 32); // should be same length as mod
				let result = BigUint::from_bytes_be(&output[..]);
				let expected = BigUint::parse_bytes(b"10055", 10).unwrap();
				assert_eq!(result, expected);
			},
			Err(_) => {
				panic!("Modexp::execute() returned error"); // TODO: how to pass error on?
			}
		}
	}

	#[test]
	fn test_large_computation() {

		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000020\
			0000000000000000000000000000000000000000000000000000000000000020\
			03\
			fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2e\
			fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f")
			.expect("Decode failed");

		let cost: usize = 1;

		match Modexp::execute(&input, cost) {
			Ok((_, output)) => {
				assert_eq!(output.len(), 32); // should be same length as mod
				let result = BigUint::from_bytes_be(&output[..]);
				let expected = BigUint::parse_bytes(b"1", 10).unwrap();
				assert_eq!(result, expected);
			},
			Err(_) => {
				panic!("Modexp::execute() returned error"); // TODO: how to pass error on?
			}
		}
	}
}
