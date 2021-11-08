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
use core::cmp::min;
use fp_evm::{
	Context, ExitError, ExitSucceed, LinearCostPrecompile, PrecompileFailure, PrecompileOutput,
	PrecompileResult,
};

/// The identity precompile.
pub struct Identity;

impl LinearCostPrecompile for Identity {
	const BASE: u64 = 15;
	const WORD: u64 = 3;

	fn execute(
		input: &[u8],
		target_gas: u64,
		_context: &Context,
		_is_static: bool,
	) -> PrecompileResult {
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: target_gas,
			output: input.to_vec(),
			logs: Default::default(),
		})
	}
}

/// The ecrecover precompile.
pub struct ECRecover;

impl LinearCostPrecompile for ECRecover {
	const BASE: u64 = 3000;
	const WORD: u64 = 0;

	fn execute(
		i: &[u8],
		target_gas: u64,
		_context: &Context,
		_is_static: bool,
	) -> PrecompileResult {
		let mut input = [0u8; 128];
		input[..min(i.len(), 128)].copy_from_slice(&i[..min(i.len(), 128)]);

		let mut msg = [0u8; 32];
		let mut sig = [0u8; 65];

		msg[0..32].copy_from_slice(&input[0..32]);
		sig[0..32].copy_from_slice(&input[64..96]);
		sig[32..64].copy_from_slice(&input[96..128]);
		sig[64] = input[63];

		let result = match sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg) {
			Ok(pubkey) => {
				let mut address = sp_io::hashing::keccak_256(&pubkey);
				address[0..12].copy_from_slice(&[0u8; 12]);
				address.to_vec()
			}
			Err(_) => [0u8; 0].to_vec(),
		};

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: target_gas,
			output: result,
			logs: Default::default(),
		})
	}
}

/// The ripemd precompile.
pub struct Ripemd160;

impl LinearCostPrecompile for Ripemd160 {
	const BASE: u64 = 600;
	const WORD: u64 = 120;

	fn execute(input: &[u8], _cost: u64, _context: &Context, _is_static: bool) -> PrecompileResult {
		use ripemd160::Digest;

		let mut ret = [0u8; 32];
		ret[12..32].copy_from_slice(&ripemd160::Ripemd160::digest(input));
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: _cost,
			output: ret.to_vec(),
			logs: Default::default(),
		})
	}
}

/// The sha256 precompile.
pub struct Sha256;

impl LinearCostPrecompile for Sha256 {
	const BASE: u64 = 60;
	const WORD: u64 = 12;

	fn execute(input: &[u8], _cost: u64, _context: &Context, _is_static: bool) -> PrecompileResult {
		let ret = sp_io::hashing::sha2_256(input);
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: _cost,
			output: ret.to_vec(),
			logs: Default::default(),
		})
	}
}

/// The ECRecoverPublicKey precompile.
/// Similar to ECRecover, but returns the pubkey (not the corresponding Ethereum address)
pub struct ECRecoverPublicKey;

impl LinearCostPrecompile for ECRecoverPublicKey {
	const BASE: u64 = 3000;
	const WORD: u64 = 0;

	fn execute(i: &[u8], _cost: u64, _context: &Context, _is_static: bool) -> PrecompileResult {
		let mut input = [0u8; 128];
		input[..min(i.len(), 128)].copy_from_slice(&i[..min(i.len(), 128)]);

		let mut msg = [0u8; 32];
		let mut sig = [0u8; 65];

		msg[0..32].copy_from_slice(&input[0..32]);
		sig[0..32].copy_from_slice(&input[64..96]);
		sig[32..64].copy_from_slice(&input[96..128]);
		sig[64] = input[63];

		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg).map_err(|_| {
			PrecompileFailure::Error {
				exit_status: ExitError::Other("Public key recover failed".into()),
			}
		})?;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: _cost,
			output: pubkey.to_vec(),
			logs: Default::default(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_evm_test_vector_support::test_precompile_test_vectors;

	// TODO: this fails on the test "InvalidHighV-bits-1" where it is expected to return ""
	#[test]
	fn process_consensus_tests_for_ecrecover() -> std::result::Result<(), String> {
		test_precompile_test_vectors::<ECRecover>("../testdata/ecRecover.json")?;
		Ok(())
	}

	#[test]
	fn process_consensus_tests_for_sha256() -> std::result::Result<(), String> {
		test_precompile_test_vectors::<Sha256>("../testdata/common_sha256.json")?;
		Ok(())
	}

	#[test]
	fn process_consensus_tests_for_ripemd160() -> std::result::Result<(), String> {
		test_precompile_test_vectors::<Ripemd160>("../testdata/common_ripemd.json")?;
		Ok(())
	}
}
