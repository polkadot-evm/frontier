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

use alloc::vec;
use core::marker::PhantomData;
use frame_benchmarking::v2::*;
use sp_runtime::Vec;

// Import precompile implementations
use pallet_evm_precompile_sha3fips::{Sha3FIPS256, Sha3FIPS512};

pub struct Pallet<T: Config>(PhantomData<T>);
pub trait Config: frame_system::Config {}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn sha3_fips_256(n: Linear<1, 4_096>) -> Result<(), BenchmarkError> {
		// Deterministic preimage content of requested size
		let mut input: Vec<u8> = vec![0; n as usize];
		input.resize(n as usize, 0u8);
		for (i, b) in input.iter_mut().enumerate() {
			*b = (i as u8).wrapping_mul(31).wrapping_add(7);
		}

		#[block]
		{
			Sha3FIPS256::<(), ()>::execute_inner(&input, 0)
				.expect("Failed to execute sha3 fips 256");
		}

		Ok(())
	}

	#[benchmark]
	fn sha3_fips_512(n: Linear<1, 4_096>) -> Result<(), BenchmarkError> {
		// Deterministic preimage content of requested size
		let mut input: Vec<u8> = vec![0; n as usize];
		input.resize(n as usize, 0u8);
		for (i, b) in input.iter_mut().enumerate() {
			*b = (i as u8).wrapping_mul(17).wrapping_add(13);
		}

		#[block]
		{
			Sha3FIPS512::<(), ()>::execute_inner(&input, 0)
				.expect("Failed to execute sha3 fips 512");
		}

		Ok(())
	}
}
