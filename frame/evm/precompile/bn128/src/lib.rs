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
use sp_core::U256;
use fp_evm::Precompile;
use evm::{ExitSucceed, ExitError, Context};

fn read_fr(input: &[u8], start_inx: usize) -> Result<bn::Fr, ExitError> {
	if input.len() < start_inx + 32 {
		return Err(ExitError::Other("Input not long enough".into()));
	}

	bn::Fr::from_slice(&input[start_inx..(start_inx + 32)]).map_err(|_| ExitError::Other("Invalid field element".into()))
}

fn read_point(input: &[u8], start_inx: usize) -> Result<bn::G1, ExitError> {
	use bn::{Fq, AffineG1, G1, Group};

	if input.len() < start_inx + 64 {
		return Err(ExitError::Other("Input not long enough".into()));
	}

	let px = Fq::from_slice(&input[start_inx..(start_inx + 32)]).map_err(|_| ExitError::Other("Invalid point x coordinate".into()))?;
	let py = Fq::from_slice(&input[(start_inx + 32)..(start_inx + 64)]).map_err(|_| ExitError::Other("Invalid point y coordinate".into()))?;
	Ok(
		if px == Fq::zero() && py == Fq::zero() {
			G1::zero()
		} else {
			AffineG1::new(px, py).map_err(|_| ExitError::Other("Invalid curve point".into()))?.into()
		}
	)
}

/// The Bn128Add builtin
pub struct Bn128Add;

impl Bn128Add {
	const GAS_COST: u64 = 150; // https://eips.ethereum.org/EIPS/eip-1108
}

impl Precompile for Bn128Add {

	fn execute(
		input: &[u8],
		_target_gas: Option<u64>,
		_context: &Context,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, u64), ExitError> {
		use bn::AffineG1;

		let p1 = read_point(input, 0)?;
		let p2 = read_point(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p1 + p2) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).map_err(|_| ExitError::Other("Cannot fail since 0..32 is 32-byte length".into()))?;
			sum.y().to_big_endian(&mut buf[32..64]).map_err(|_| ExitError::Other("Cannot fail since 32..64 is 32-byte length".into()))?;
		}

		Ok((ExitSucceed::Returned, buf.to_vec(), Bn128Add::GAS_COST))
	}
}

/// The Bn128Mul builtin
pub struct Bn128Mul;

impl Bn128Mul {
	const GAS_COST: u64 = 6_000; // https://eips.ethereum.org/EIPS/eip-1108
}

impl Precompile for Bn128Mul {

	fn execute(
		input: &[u8],
		_target_gas: Option<u64>,
		_context: &Context,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, u64), ExitError> {
		use bn::AffineG1;

		let p = read_point(input, 0)?;
		let fr = read_fr(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p * fr) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).map_err(|_| ExitError::Other("Cannot fail since 0..32 is 32-byte length".into()))?;
			sum.y().to_big_endian(&mut buf[32..64]).map_err(|_| ExitError::Other("Cannot fail since 32..64 is 32-byte length".into()))?;
		}

		Ok((ExitSucceed::Returned, buf.to_vec(), Bn128Mul::GAS_COST))
	}
}

/// The Bn128Pairing builtin
pub struct Bn128Pairing;

impl Bn128Pairing {
	// https://eips.ethereum.org/EIPS/eip-1108
	const BASE_GAS_COST: u64 = 45_000;
	const GAS_COST_PER_PAIRING: u64 = 34_000;
}

impl Precompile for Bn128Pairing {

	fn execute(
		input: &[u8],
		target_gas: Option<u64>,
		_context: &Context,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, u64), ExitError> {
		use bn::{AffineG1, AffineG2, Fq, Fq2, pairing_batch, G1, G2, Gt, Group};

		let (ret_val, gas_cost) = if input.is_empty() {
			(U256::one(), Bn128Pairing::BASE_GAS_COST)
		} else {
			// (a, b_a, b_b - each 64-byte affine coordinates)
			let elements = input.len() / 192;

			let gas_cost: u64 =
				Bn128Pairing::BASE_GAS_COST + (elements as u64 * Bn128Pairing::GAS_COST_PER_PAIRING);
			if let Some(gas_left) = target_gas {
				if gas_left < gas_cost {
					return Err(ExitError::OutOfGas);
				}
			}

			let mut vals = Vec::new();
			for idx in 0..elements {
				let a_x = Fq::from_slice(&input[idx*192..idx*192+32])
					.map_err(|_| ExitError::Other("Invalid a argument x coordinate".into()))?;

				let a_y = Fq::from_slice(&input[idx*192+32..idx*192+64])
					.map_err(|_| ExitError::Other("Invalid a argument y coordinate".into()))?;

				let b_a_y = Fq::from_slice(&input[idx*192+64..idx*192+96])
					.map_err(|_| ExitError::Other("Invalid b argument imaginary coeff x coordinate".into()))?;

				let b_a_x = Fq::from_slice(&input[idx*192+96..idx*192+128])
					.map_err(|_| ExitError::Other("Invalid b argument imaginary coeff y coordinate".into()))?;

				let b_b_y = Fq::from_slice(&input[idx*192+128..idx*192+160])
					.map_err(|_| ExitError::Other("Invalid b argument real coeff x coordinate".into()))?;

				let b_b_x = Fq::from_slice(&input[idx*192+160..idx*192+192])
					.map_err(|_| ExitError::Other("Invalid b argument real coeff y coordinate".into()))?;

				let b_a = Fq2::new(b_a_x, b_a_y);
				let b_b = Fq2::new(b_b_x, b_b_y);
				let b = if b_a.is_zero() && b_b.is_zero() {
					G2::zero()
				} else {
					G2::from(AffineG2::new(b_a, b_b).map_err(|_| ExitError::Other("Invalid b argument - not on curve".into()))?)
				};
				let a = if a_x.is_zero() && a_y.is_zero() {
					G1::zero()
				} else {
					G1::from(AffineG1::new(a_x, a_y).map_err(|_| ExitError::Other("Invalid a argument - not on curve".into()))?)
				};
				vals.push((a, b));
			};

			let mul = pairing_batch(&vals);

			if mul == Gt::one() {
				(U256::one(), gas_cost)
			} else {
				(U256::zero(), gas_cost)
			}
		};

		let mut buf = [0u8; 32];
		ret_val.to_big_endian(&mut buf);

		Ok((ExitSucceed::Returned, buf.to_vec(), gas_cost))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_evm_test_vector_support::test_precompile_test_vectors;

	#[test]
	fn process_consensus_tests_for_add() -> std::result::Result<(), String> {
		test_precompile_test_vectors::<Bn128Add>("../testdata/common_bnadd.json")?;
		Ok(())
	}

	#[test]
	fn process_consensus_tests_for_mul() -> std::result::Result<(), String> {
		test_precompile_test_vectors::<Bn128Mul>("../testdata/common_bnmul.json")?;
		Ok(())
	}

	#[test]
	fn process_consensus_tests_for_pair() -> std::result::Result<(), String> {
		test_precompile_test_vectors::<Bn128Pairing>("../testdata/common_bnpair.json")?;
		Ok(())
	}
}
