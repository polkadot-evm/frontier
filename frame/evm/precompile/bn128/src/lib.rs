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
use fp_evm::LinearCostPrecompile;
use evm::{ExitSucceed, ExitError};

fn read_fr(input: &[u8], start_inx: usize) -> Result<bn::Fr, ExitError> {
	bn::Fr::from_slice(&input[start_inx..(start_inx + 32)]).map_err(|_| ExitError::Other("Invalid field element".into()))
}

fn read_point(input: &[u8], start_inx: usize) -> Result<bn::G1, ExitError> {
	use bn::{Fq, AffineG1, G1, Group};

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

impl LinearCostPrecompile for Bn128Add {
	const BASE: u64 = 15;
	const WORD: u64 = 3;

	fn execute(
		input: &[u8],
		_: u64,
	) -> core::result::Result<(ExitSucceed, Vec<u8>), ExitError> {
		use bn::AffineG1;

		let p1 = read_point(input, 0)?;
		let p2 = read_point(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p1 + p2) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).map_err(|_| ExitError::Other("Cannot fail since 0..32 is 32-byte length".into()))?;
			sum.y().to_big_endian(&mut buf[32..64]).map_err(|_| ExitError::Other("Cannot fail since 32..64 is 32-byte length".into()))?;
		}

		Ok((ExitSucceed::Returned, buf.to_vec()))
	}
}

/// The Bn128Mul builtin
pub struct Bn128Mul;

impl LinearCostPrecompile for Bn128Mul {
	const BASE: u64 = 15;
	const WORD: u64 = 3;

	fn execute(
		input: &[u8],
		_: u64,
	) -> core::result::Result<(ExitSucceed, Vec<u8>), ExitError> {
		use bn::AffineG1;

		let p = read_point(input, 0)?;
		let fr = read_fr(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p * fr) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).map_err(|_| ExitError::Other("Cannot fail since 0..32 is 32-byte length".into()))?;
			sum.y().to_big_endian(&mut buf[32..64]).map_err(|_| ExitError::Other("Cannot fail since 32..64 is 32-byte length".into()))?;
		}

		Ok((ExitSucceed::Returned, buf.to_vec()))
	}
}

/// The Bn128Pairing builtin
pub struct Bn128Pairing;

impl LinearCostPrecompile for Bn128Pairing {
	const BASE: u64 = 15;
	const WORD: u64 = 3;

	fn execute(
		input: &[u8],
		_: u64,
	) -> core::result::Result<(ExitSucceed, Vec<u8>), ExitError> {
		use bn::{AffineG1, AffineG2, Fq, Fq2, pairing_batch, G1, G2, Gt, Group};

		let ret_val = if input.is_empty() {
			U256::one()
		} else {
			// (a, b_a, b_b - each 64-byte affine coordinates)
			let elements = input.len() / 192;
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
				U256::one()
			} else {
				U256::zero()
			}
		};

		let mut buf = [0u8; 32];
		ret_val.to_big_endian(&mut buf);

		Ok((ExitSucceed::Returned, buf.to_vec()))
	}
}
