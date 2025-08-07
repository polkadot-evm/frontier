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

use alloc::format;
use core::marker::PhantomData;
use curve25519_dalek::{RistrettoPoint, scalar::Scalar};
use frame_benchmarking::v2::*;
use sha2::Sha512;
use sp_runtime::Vec;

// Import existing precompile implementations
use pallet_evm_precompile_curve25519::{Curve25519Add, Curve25519ScalarMul};

pub struct Pallet<T: Config>(PhantomData<T>);
pub trait Config: frame_system::Config {}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn curve25519_add_n_points(n: Linear<1, 10>) -> Result<(), BenchmarkError> {
		// Encode N points into a single buffer
		let mut points = Vec::new();
		for i in 0..n {
			points.extend(RistrettoPoint::hash_from_bytes::<Sha512>(format!("point_{}", i).as_bytes()).compress().to_bytes().to_vec());
		}

		#[block]
        {
            Curve25519Add::<(), ()>::execute_inner(&points, 0)
                .expect("Failed to execute curve25519 add");
        }

		Ok(())
	}

	#[benchmark]
	fn curve25519_scaler_mul() -> Result<(), BenchmarkError> {
		// Encode input (scalar - 32 bytes, point - 32 bytes)
		let mut input = [0; 64];
		input[0..32].copy_from_slice(&Scalar::from(1234567890u64).to_bytes());
		input[32..64].copy_from_slice(&RistrettoPoint::hash_from_bytes::<Sha512>("point_0".as_bytes()).compress().to_bytes());

		#[block]
        {
            Curve25519ScalarMul::<(), ()>::execute_inner(&input, 0)
                .expect("Failed to execute curve25519 add");
        }

		Ok(())
	}
}

