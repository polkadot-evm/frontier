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
#![warn(unused_crate_dependencies)]

extern crate alloc;

use alloc::vec::Vec;
use core::marker::PhantomData;
use curve25519_dalek::{
	ristretto::{CompressedRistretto, RistrettoPoint},
	scalar::Scalar,
	traits::Identity,
};
use fp_evm::{
	ExitError, ExitSucceed, Precompile, PrecompileFailure, PrecompileHandle, PrecompileOutput,
	PrecompileResult,
};
use frame_support::weights::Weight;
use pallet_evm::GasWeightMapping;

// Weight provider trait expected by these precompiles. Implementations should return Substrate Weights.
pub trait WeightInfo {
	fn curve25519_add_n_points(n: u32) -> Weight;
	fn curve25519_scaler_mul() -> Weight;
}

// Default weights from benchmarks run on a laptop, do not use them in production !
impl WeightInfo for () {
	/// The range of component `n` is `[1, 10]`.
	fn curve25519_add_n_points(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 10_000_000 picoseconds.
		Weight::from_parts(5_399_134, 0)
			.saturating_add(Weight::from_parts(0, 0))
			// Standard Error: 8_395
			.saturating_add(Weight::from_parts(5_153_957, 0).saturating_mul(n.into()))
	}
	fn curve25519_scaler_mul() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 81_000_000 picoseconds.
		Weight::from_parts(87_000_000, 0)
			.saturating_add(Weight::from_parts(0, 0))
	}
}

// Adds at most 10 curve25519 points and returns the CompressedRistretto bytes representation
pub struct Curve25519Add<R, WI>(PhantomData<(R, WI)>);

impl<R, WI> Precompile for Curve25519Add<R, WI>
where
	R: pallet_evm::Config,
	WI: WeightInfo,
{
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		let n_points = (handle.input().len() / 32) as u32;
		let weight = WI::curve25519_add_n_points(n_points);
		let gas = R::GasWeightMapping::weight_to_gas(weight);
		handle.record_cost(gas)?;
		let (exit_status, output) = Self::execute_inner(handle.input(), gas)?;
		Ok(PrecompileOutput {
			exit_status,
			output,
		})
	}
}

impl<R, WI> Curve25519Add<R, WI>
where
	WI: WeightInfo,
{
	pub fn execute_inner(
		input: &[u8],
		_: u64,
	) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		if input.len() % 32 != 0 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("input must contain multiple of 32 bytes".into()),
			});
		};

		if input.len() > 320 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(
					"input cannot be greater than 320 bytes (10 compressed points)".into(),
				),
			});
		};

		let mut points = Vec::new();
		let mut temp_buf = <&[u8]>::clone(&input);
		while !temp_buf.is_empty() {
			let mut buf = [0; 32];
			buf.copy_from_slice(&temp_buf[0..32]);
			let point = CompressedRistretto(buf);
			points.push(point);
			temp_buf = &temp_buf[32..];
		}

		let sum = points.iter().try_fold(
			RistrettoPoint::identity(),
			|acc, point| -> Result<RistrettoPoint, PrecompileFailure> {
				let pt = point.decompress().ok_or_else(|| PrecompileFailure::Error {
					exit_status: ExitError::Other("invalid compressed Ristretto point".into()),
				})?;
				Ok(acc + pt)
			},
		)?;

		Ok((ExitSucceed::Returned, sum.compress().to_bytes().to_vec()))
	}
}

// Multiplies a scalar field element with an elliptic curve point
pub struct Curve25519ScalarMul<R, WI>(PhantomData<(R, WI)>);

impl<R, WI> Precompile for Curve25519ScalarMul<R, WI>
where
	R: pallet_evm::Config,
	WI: WeightInfo,
{
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		let weight = WI::curve25519_scaler_mul();
		let gas = R::GasWeightMapping::weight_to_gas(weight);
		handle.record_cost(gas)?;
		let (exit_status, output) = Self::execute_inner(handle.input(), gas)?;
		Ok(PrecompileOutput {
			exit_status,
			output,
		})
	}
}

impl<R, WI> Curve25519ScalarMul<R, WI>
where
	WI: WeightInfo,
{
	pub fn execute_inner(
		input: &[u8],
		_: u64,
	) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		if input.len() != 64 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(
					"input must contain 64 bytes (scalar - 32 bytes, point - 32 bytes)".into(),
				),
			});
		};

		// first 32 bytes is for the scalar value
		let mut scalar_buf = [0; 32];
		scalar_buf.copy_from_slice(&input[0..32]);
		let scalar = Scalar::from_bytes_mod_order(scalar_buf);

		// second 32 bytes is for the compressed ristretto point bytes
		let mut pt_buf = [0; 32];
		pt_buf.copy_from_slice(&input[32..64]);
		let point =
			CompressedRistretto(pt_buf)
				.decompress()
				.ok_or_else(|| PrecompileFailure::Error {
					exit_status: ExitError::Other("invalid compressed Ristretto point".into()),
				})?;

		let scalar_mul = scalar * point;
		Ok((
			ExitSucceed::Returned,
			scalar_mul.compress().to_bytes().to_vec(),
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use curve25519_dalek::constants;

	#[test]
	fn test_sum() -> Result<(), PrecompileFailure> {
		let s1 = Scalar::from(999u64);
		let p1 = constants::RISTRETTO_BASEPOINT_POINT * s1;

		let s2 = Scalar::from(333u64);
		let p2 = constants::RISTRETTO_BASEPOINT_POINT * s2;

		let vec = vec![p1, p2];
		let mut input = vec![];
		input.extend_from_slice(&p1.compress().to_bytes());
		input.extend_from_slice(&p2.compress().to_bytes());

		let sum: RistrettoPoint = vec.iter().sum();
		let cost: u64 = 1;

		match Curve25519Add::<(), ()>::execute_inner(&input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, sum.compress().to_bytes());
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn test_empty() -> Result<(), PrecompileFailure> {
		// Test that sum works for the empty iterator
		let input = vec![];

		let cost: u64 = 1;

		match Curve25519Add::<(), ()>::execute_inner(&input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, RistrettoPoint::identity().compress().to_bytes());
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn test_scalar_mul() -> Result<(), PrecompileFailure> {
		let s1 = Scalar::from(999u64);
		let s2 = Scalar::from(333u64);
		let p1 = constants::RISTRETTO_BASEPOINT_POINT * s1;
		let p2 = constants::RISTRETTO_BASEPOINT_POINT * s2;

		let mut input = vec![];
		input.extend_from_slice(&s1.to_bytes());
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes());

		let cost: u64 = 1;

		match Curve25519ScalarMul::<(), ()>::execute_inner(&input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, p1.compress().to_bytes());
				assert_ne!(out, p2.compress().to_bytes());
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn test_scalar_mul_empty_error() -> Result<(), PrecompileFailure> {
		let input = vec![];

		let cost: u64 = 1;

		match Curve25519ScalarMul::<(), ()>::execute_inner(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other(
							"input must contain 64 bytes (scalar - 32 bytes, point - 32 bytes)"
								.into()
						)
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_point_addition_bad_length() -> Result<(), PrecompileFailure> {
		let input: Vec<u8> = [0u8; 33].to_vec();

		let cost: u64 = 1;

		match Curve25519Add::<(), ()>::execute_inner(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other(
							"input must contain multiple of 32 bytes".into()
						)
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_point_addition_too_many_points() -> Result<(), PrecompileFailure> {
		let mut input = vec![];
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 1
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 2
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 3
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 4
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 5
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 6
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 7
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 8
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 9
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 10
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 11

		let cost: u64 = 1;

		match Curve25519Add::<(), ()>::execute_inner(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other(
							"input cannot be greater than 320 bytes (10 compressed points)".into()
						)
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_point_addition_invalid_point() -> Result<(), PrecompileFailure> {
		// Create an invalid compressed Ristretto point
		// Using a pattern that's definitely invalid for Ristretto compression
		let mut invalid_point = [0u8; 32];
		invalid_point[31] = 0xFF; // Set the last byte to 0xFF, which is invalid for Ristretto
		let mut input = vec![];
		input.extend_from_slice(&invalid_point);

		let cost: u64 = 1;

		match Curve25519Add::<(), ()>::execute_inner(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work with invalid point");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other("invalid compressed Ristretto point".into())
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_scalar_mul_invalid_point() -> Result<(), PrecompileFailure> {
		// Create an invalid compressed Ristretto point
		// Using a pattern that's definitely invalid for Ristretto compression
		let mut invalid_point = [0u8; 32];
		invalid_point[31] = 0xFF; // Set the last byte to 0xFF, which is invalid for Ristretto
		let scalar = [1u8; 32];
		let mut input = vec![];
		input.extend_from_slice(&scalar);
		input.extend_from_slice(&invalid_point);

		let cost: u64 = 1;

		match Curve25519ScalarMul::<(), ()>::execute_inner(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work with invalid point");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other("invalid compressed Ristretto point".into())
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_point_addition_mixed_valid_invalid() -> Result<(), PrecompileFailure> {
		// Create a mix of valid and invalid points
		let valid_point = constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes();
		let mut invalid_point = [0u8; 32];
		invalid_point[31] = 0xFF; // Set the last byte to 0xFF, which is invalid for Ristretto
		let mut input = vec![];
		input.extend_from_slice(&valid_point);
		input.extend_from_slice(&invalid_point);

		let cost: u64 = 1;

		match Curve25519Add::<(), ()>::execute_inner(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work with invalid point");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other("invalid compressed Ristretto point".into())
					}
				);
				Ok(())
			}
		}
	}
}
