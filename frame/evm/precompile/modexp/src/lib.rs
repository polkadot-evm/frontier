// This file is part of Frontier.

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

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::comparison_chain)]
#![warn(unused_crate_dependencies)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use core::cmp::max;

use num::{BigUint, FromPrimitive, Integer, One, ToPrimitive, Zero};

use fp_evm::{
	ExitError, ExitSucceed, Precompile, PrecompileFailure, PrecompileHandle, PrecompileOutput,
	PrecompileResult,
};

pub struct Modexp;

// EIP-7883 Gas Pricing Constants
// https://eips.ethereum.org/EIPS/eip-7883

/// Minimum gas cost for ModExp operations (EIP-7883: increased from 200 to 500).
const MIN_GAS_COST: u64 = 500;

/// Threshold in bytes for applying the complexity floor vs scaled complexity.
/// Inputs with max(base_length, mod_length) <= this value use the floor.
const COMPLEXITY_THRESHOLD: u64 = 32;

/// Minimum multiplication complexity for small inputs (<= 32 bytes).
/// EIP-7883 introduces this floor to address underpricing of small operations.
const MIN_COMPLEXITY: u64 = 16;

/// Multiplier applied to word-squared complexity for large inputs (> 32 bytes).
/// EIP-7883 doubles the complexity calculation for larger operations.
const COMPLEXITY_MULTIPLIER: u64 = 2;

/// Multiplier for iteration count when exponent length exceeds 32 bytes.
/// EIP-7883 doubles this from 8 to 16 to better reflect computation costs.
const ITERATION_MULTIPLIER: u64 = 16;

/// Gas multiplier for even moduli to compensate for slower computation.
/// CVE-2023-28431: num-bigint uses Montgomery multiplication for odd moduli (fast)
/// but plain power algorithm for even moduli (~20x slower).
/// See: https://github.com/polkadot-evm/frontier/security/advisories/GHSA-fcmm-54jp-7vf6
const EVEN_MODULUS_MULTIPLIER: u64 = 20;

/// EIP-7823: Maximum input size limit for MODEXP (8192 bits = 1024 bytes).
/// Each length field (base, exponent, modulus) MUST not exceed this limit.
/// Inputs exceeding this limit consume all available gas and return an error.
/// https://eips.ethereum.org/EIPS/eip-7823
const EIP7823_INPUT_SIZE_LIMIT: u32 = 1024;

// Calculate gas cost according to EIP-7883:
// https://eips.ethereum.org/EIPS/eip-7883
fn calculate_gas_cost(
	base_length: u64,
	mod_length: u64,
	exponent: &BigUint,
	exponent_bytes: &[u8],
	mod_is_even: bool,
) -> u64 {
	fn calculate_multiplication_complexity(base_length: u64, mod_length: u64) -> u64 {
		let max_length = max(base_length, mod_length);

		if max_length <= COMPLEXITY_THRESHOLD {
			return MIN_COMPLEXITY;
		}

		let mut words = max_length / 8;
		if max_length % 8 > 0 {
			words += 1;
		}

		// Note: can't overflow because we take words to be some u64 value / 8, which is
		// necessarily less than sqrt(u64::MAX).
		// Additionally, both base_length and mod_length are bounded to 1024, so this has
		// an upper bound of roughly COMPLEXITY_MULTIPLIER * (1024 / 8) squared
		COMPLEXITY_MULTIPLIER * words * words
	}

	fn calculate_iteration_count(exponent: &BigUint, exponent_bytes: &[u8]) -> u64 {
		let mut iteration_count: u64 = 0;
		let exp_length = exponent_bytes.len() as u64;

		if exp_length <= 32 && exponent.is_zero() {
			iteration_count = 0;
		} else if exp_length <= 32 {
			iteration_count = exponent.bits() - 1;
		} else if exp_length > 32 {
			// from the EIP spec:
			// (ITERATION_MULTIPLIER * (exp_length - 32)) + ((exponent & (2**256 - 1)).bit_length() - 1)
			//
			// Notes:
			// * exp_length is bounded to 1024 and is > 32
			// * exponent can be zero, so we subtract 1 after adding the other terms (whose sum
			//   must be > 0)
			// * the addition can't overflow because the terms are both capped at roughly
			//   ITERATION_MULTIPLIER * max size of exp_length (1024)
			// * the EIP spec is written in python, in which (exponent & (2**256 - 1)) takes the
			//   FIRST 32 bytes. However this `BigUint` `&` operator takes the LAST 32 bytes.
			//   We thus instead take the bytes manually.
			let exponent_head = BigUint::from_bytes_be(&exponent_bytes[..32]);

			iteration_count = (ITERATION_MULTIPLIER * (exp_length - 32)) + exponent_head.bits() - 1;
		}

		max(iteration_count, 1)
	}

	let multiplication_complexity = calculate_multiplication_complexity(base_length, mod_length);
	let iteration_count = calculate_iteration_count(exponent, exponent_bytes);
	let gas = max(MIN_GAS_COST, multiplication_complexity * iteration_count);

	gas.saturating_mul(if mod_is_even {
		EVEN_MODULUS_MULTIPLIER
	} else {
		1
	})
}

/// Copy bytes from input to target.
fn read_input(source: &[u8], target: &mut [u8], source_offset: &mut usize) {
	// We move the offset by the len of the target, regardless of what we
	// actually copy.
	let offset = *source_offset;
	*source_offset += target.len();

	// Out of bounds, nothing to copy.
	if source.len() <= offset {
		return;
	}

	// Find len to copy up to target len, but not out of bounds.
	let len = core::cmp::min(target.len(), source.len() - offset);
	target[..len].copy_from_slice(&source[offset..][..len]);
}

// ModExp expects the following as inputs:
// 1) 32 bytes expressing the length of base
// 2) 32 bytes expressing the length of exponent
// 3) 32 bytes expressing the length of modulus
// 4) base, size as described above
// 5) exponent, size as described above
// 6) modulus, size as described above
//
//
// NOTE: input sizes are bound to 1024 bytes, with the expectation
//       that gas limits would be applied before actual computation.
//
//       maximum stack size will also prevent abuse.
//
//       see: https://eips.ethereum.org/EIPS/eip-198

impl Precompile for Modexp {
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		let input = handle.input();
		let mut input_offset = 0;

		// Yellowpaper: whenever the input is too short, the missing bytes are
		// considered to be zero.
		let mut base_len_buf = [0u8; 32];
		read_input(input, &mut base_len_buf, &mut input_offset);
		let mut exp_len_buf = [0u8; 32];
		read_input(input, &mut exp_len_buf, &mut input_offset);
		let mut mod_len_buf = [0u8; 32];
		read_input(input, &mut mod_len_buf, &mut input_offset);

		// EIP-7823: Maximum input size limit (8192 bits = 1024 bytes)
		let max_size_big =
			BigUint::from_u32(EIP7823_INPUT_SIZE_LIMIT).expect("can't create BigUint");

		let base_len_big = BigUint::from_bytes_be(&base_len_buf);
		if base_len_big > max_size_big {
			// EIP-7823: Consume all remaining gas before returning error
			handle.record_cost(handle.remaining_gas())?;
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("EIP-7823: base length exceeds 1024 bytes".into()),
			});
		}

		let exp_len_big = BigUint::from_bytes_be(&exp_len_buf);
		if exp_len_big > max_size_big {
			// EIP-7823: Consume all remaining gas before returning error
			handle.record_cost(handle.remaining_gas())?;
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(
					"EIP-7823: exponent length exceeds 1024 bytes".into(),
				),
			});
		}

		let mod_len_big = BigUint::from_bytes_be(&mod_len_buf);
		if mod_len_big > max_size_big {
			// EIP-7823: Consume all remaining gas before returning error
			handle.record_cost(handle.remaining_gas())?;
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("EIP-7823: modulus length exceeds 1024 bytes".into()),
			});
		}

		// bounds check handled above
		let base_len = base_len_big.to_usize().expect("base_len out of bounds");
		let exp_len = exp_len_big.to_usize().expect("exp_len out of bounds");
		let mod_len = mod_len_big.to_usize().expect("mod_len out of bounds");

		// if mod_len is 0 output must be empty
		if mod_len == 0 {
			return Ok(PrecompileOutput {
				exit_status: ExitSucceed::Returned,
				output: vec![],
			});
		}

		// Gas formula allows arbitrary large exp_len when base and modulus are empty, so we need to handle empty base first.
		let r = if base_len == 0 && mod_len == 0 {
			handle.record_cost(MIN_GAS_COST)?;
			BigUint::zero()
		} else {
			// read the numbers themselves.
			let mut base_buf = vec![0u8; base_len];
			read_input(input, &mut base_buf, &mut input_offset);
			let base = BigUint::from_bytes_be(&base_buf);

			let mut exp_buf = vec![0u8; exp_len];
			read_input(input, &mut exp_buf, &mut input_offset);
			let exponent = BigUint::from_bytes_be(&exp_buf);

			let mut mod_buf = vec![0u8; mod_len];
			read_input(input, &mut mod_buf, &mut input_offset);
			let modulus = BigUint::from_bytes_be(&mod_buf);

			// do our gas accounting
			let gas_cost = calculate_gas_cost(
				base_len as u64,
				mod_len as u64,
				&exponent,
				&exp_buf,
				modulus.is_even(),
			);

			handle.record_cost(gas_cost)?;

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
			Ok(PrecompileOutput {
				exit_status: ExitSucceed::Returned,
				output: bytes.to_vec(),
			})
		} else if bytes.len() < mod_len {
			let mut ret = Vec::with_capacity(mod_len);
			ret.extend(core::iter::repeat_n(0, mod_len - bytes.len()));
			ret.extend_from_slice(&bytes[..]);
			Ok(PrecompileOutput {
				exit_status: ExitSucceed::Returned,
				output: ret.to_vec(),
			})
		} else {
			Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("failed".into()),
			})
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	extern crate hex;
	use fp_evm::Context;
	use pallet_evm_test_vector_support::{test_precompile_test_vectors, MockHandle};

	#[test]
	fn process_consensus_tests() -> Result<(), String> {
		test_precompile_test_vectors::<Modexp>("../testdata/modexp_eip7883.json")?;
		Ok(())
	}

	#[test]
	fn test_empty_input() {
		let input = Vec::new();

		let cost: u64 = 1;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match Modexp::execute(&mut handle) {
			Ok(precompile_result) => {
				assert_eq!(precompile_result.output.len(), 0);
			}
			Err(_) => {
				panic!("Modexp::execute() returned error"); // TODO: how to pass error on?
			}
		}
	}

	#[test]
	fn test_insufficient_input() {
		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001",
		)
		.expect("Decode failed");

		let cost: u64 = 1;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match Modexp::execute(&mut handle) {
			Ok(precompile_result) => {
				assert_eq!(precompile_result.output.len(), 1);
				assert_eq!(precompile_result.output, vec![0x00]);
			}
			Err(_) => {
				panic!("Modexp::execute() returned error"); // TODO: how to pass error on?
			}
		}
	}

	#[test]
	fn test_excessive_input() -> Result<(), PrecompileFailure> {
		let input = hex::decode(
			"1000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001",
		)
		.expect("Decode failed");

		let cost: u64 = 100000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match Modexp::execute(&mut handle) {
			Ok(_) => {
				panic!("Test not expected to pass");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other(
							"EIP-7823: base length exceeds 1024 bytes".into()
						)
					}
				);
				// EIP-7823: Verify all gas was consumed
				assert_eq!(handle.gas_used, cost, "EIP-7823 requires all gas to be consumed");
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
			07",
		)
		.expect("Decode failed");

		// 3 ^ 5 % 7 == 5

		let cost: u64 = 100000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match Modexp::execute(&mut handle) {
			Ok(precompile_result) => {
				assert_eq!(precompile_result.output.len(), 1); // should be same length as mod
				let result = BigUint::from_bytes_be(&precompile_result.output[..]);
				let expected = BigUint::parse_bytes(b"5", 10).unwrap();
				assert_eq!(result, expected);
			}
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
			0000000000000000000000000000000000000000000000000000000000003874",
		)
		.expect("Decode failed");

		// 59999 ^ 21 % 14452 = 10055

		let cost: u64 = 100000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match Modexp::execute(&mut handle) {
			Ok(precompile_result) => {
				assert_eq!(precompile_result.output.len(), 32); // should be same length as mod
				let result = BigUint::from_bytes_be(&precompile_result.output[..]);
				let expected = BigUint::parse_bytes(b"10055", 10).unwrap();
				assert_eq!(result, expected);
			}
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
			fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f",
		)
		.expect("Decode failed");

		let cost: u64 = 100000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match Modexp::execute(&mut handle) {
			Ok(precompile_result) => {
				assert_eq!(precompile_result.output.len(), 32); // should be same length as mod
				let result = BigUint::from_bytes_be(&precompile_result.output[..]);
				let expected = BigUint::parse_bytes(b"1", 10).unwrap();
				assert_eq!(result, expected);
			}
			Err(_) => {
				panic!("Modexp::execute() returned error"); // TODO: how to pass error on?
			}
		}
	}

	#[test]
	fn test_zero_exp_with_33_length() {
		// This is a regression test which ensures that the 'iteration_count' calculation
		// in 'calculate_iteration_count' cannot underflow.
		//
		// In debug mode, this underflow could cause a panic. Otherwise, it causes N**0 to
		// be calculated at more-than-normal expense.
		//
		// TODO: cite security advisory

		let input = vec![
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
		];

		let cost: u64 = 100000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		let precompile_result =
			Modexp::execute(&mut handle).expect("Modexp::execute() returned error");

		assert_eq!(precompile_result.output.len(), 1); // should be same length as mod
		let result = BigUint::from_bytes_be(&precompile_result.output[..]);
		let expected = BigUint::parse_bytes(b"0", 10).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn test_long_exp_gas_cost_matches_specs() {
		let input = vec![
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 38, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			16, 0, 0, 0, 255, 255, 255, 2, 0, 0, 179, 0, 0, 2, 0, 0, 122, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 255, 251, 0, 0, 0, 0, 4, 38, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 16, 0, 0, 0, 255, 255, 255, 2, 0, 0, 179, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255,
			255, 255, 255, 249,
		];

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(100_000), context);

		let _ = Modexp::execute(&mut handle).expect("Modexp::execute() returned error");

		assert_eq!(handle.gas_used, 56448 * EVEN_MODULUS_MULTIPLIER);
	}

	// EIP-7823 Tests: Verify that exceeding the 1024-byte limit consumes all gas
	// https://eips.ethereum.org/EIPS/eip-7823

	#[test]
	fn test_eip7823_base_exceeds_limit_consumes_all_gas() {
		// Input with base_length = 1025 (0x401) - exceeds 1024 byte limit
		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000401\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001",
		)
		.expect("Decode failed");

		let gas_limit: u64 = 100000;
		let context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(gas_limit), context);

		let result = Modexp::execute(&mut handle);
		assert!(result.is_err());
		assert_eq!(
			result.unwrap_err(),
			PrecompileFailure::Error {
				exit_status: ExitError::Other("EIP-7823: base length exceeds 1024 bytes".into())
			}
		);
		// EIP-7823: All gas should be consumed
		assert_eq!(
			handle.gas_used, gas_limit,
			"EIP-7823 requires all gas to be consumed when base length exceeds limit"
		);
	}

	#[test]
	fn test_eip7823_exp_exceeds_limit_consumes_all_gas() {
		// Input with exp_length = 1025 (0x401) - exceeds 1024 byte limit
		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000401\
			0000000000000000000000000000000000000000000000000000000000000001",
		)
		.expect("Decode failed");

		let gas_limit: u64 = 100000;
		let context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(gas_limit), context);

		let result = Modexp::execute(&mut handle);
		assert!(result.is_err());
		assert_eq!(
			result.unwrap_err(),
			PrecompileFailure::Error {
				exit_status: ExitError::Other(
					"EIP-7823: exponent length exceeds 1024 bytes".into()
				)
			}
		);
		// EIP-7823: All gas should be consumed
		assert_eq!(
			handle.gas_used, gas_limit,
			"EIP-7823 requires all gas to be consumed when exponent length exceeds limit"
		);
	}

	#[test]
	fn test_eip7823_mod_exceeds_limit_consumes_all_gas() {
		// Input with mod_length = 1025 (0x401) - exceeds 1024 byte limit
		let input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000401",
		)
		.expect("Decode failed");

		let gas_limit: u64 = 100000;
		let context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(gas_limit), context);

		let result = Modexp::execute(&mut handle);
		assert!(result.is_err());
		assert_eq!(
			result.unwrap_err(),
			PrecompileFailure::Error {
				exit_status: ExitError::Other("EIP-7823: modulus length exceeds 1024 bytes".into())
			}
		);
		// EIP-7823: All gas should be consumed
		assert_eq!(
			handle.gas_used, gas_limit,
			"EIP-7823 requires all gas to be consumed when modulus length exceeds limit"
		);
	}

	#[test]
	fn test_eip7823_at_limit_succeeds() {
		// Input with all lengths at exactly 1024 bytes (0x400) - at the limit, should succeed
		// base_length = 1024, exp_length = 1, mod_length = 1
		// base = 2, exp = 3, mod = 5
		// 2^3 % 5 = 3
		let mut input = hex::decode(
			"0000000000000000000000000000000000000000000000000000000000000400\
			0000000000000000000000000000000000000000000000000000000000000001\
			0000000000000000000000000000000000000000000000000000000000000001",
		)
		.expect("Decode failed");

		// Append base (1024 bytes with value 2 at the end)
		let mut base = vec![0u8; 1023];
		base.push(2);
		input.extend(base);

		// Append exponent (1 byte with value 3)
		input.push(3);

		// Append modulus (1 byte with value 5)
		input.push(5);

		let gas_limit: u64 = 10000000;
		let context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(gas_limit), context);

		let result = Modexp::execute(&mut handle);
		assert!(result.is_ok(), "Input at exactly 1024 bytes should succeed");

		let output = result.unwrap().output;
		assert_eq!(output.len(), 1);
		assert_eq!(output[0], 3, "2^3 % 5 = 3");
	}
}
