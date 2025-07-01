// This file is part of Frontier.

// Copyright (C) Frontier developers.
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

//! # PolkaVM support for EVM Pallet

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

extern crate alloc;

pub mod vm;
mod weights;

use sp_core::H160;
use fp_evm::{ExitSucceed, ExitRevert, ExitError, PrecompileSet, IsPrecompileResult, PrecompileFailure, PrecompileOutput, PrecompileHandle};
use core::marker::PhantomData;

pub use self::{pallet::*, weights::WeightInfo};

pub trait ConvertPolkaVmGas {
	fn polkavm_gas_to_evm_gas(gas: polkavm::Gas) -> u64;
	fn evm_gas_to_polkavm_gas(gas: u64) -> polkavm::Gas;
}

pub struct PolkaVmSet<Inner, T>(pub Inner, PhantomData<T>);

impl<Inner, T> PolkaVmSet<Inner, T> {
	pub fn new(inner: Inner) -> Self {
		Self(inner, PhantomData)
	}
}

impl<Inner: PrecompileSet, T: Config> PrecompileSet for PolkaVmSet<Inner, T> {
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<Result<PrecompileOutput, PrecompileFailure>> {
		let code_address = handle.code_address();
		let code = pallet_evm::AccountCodes::<T>::get(code_address);
		if code[0..8] == vm::PREFIX {
			let mut run = || {
				let prepared_call: vm::PreparedCall<'_, T, _> = vm::PreparedCall::load(handle)?;
				prepared_call.call()
			};

			match run() {
				Ok(val) => {
					if val.did_revert() {
						Some(Err(PrecompileFailure::Revert {
							exit_status: ExitRevert::Reverted,
							output: val.data
						}))
					} else {
						Some(Ok(PrecompileOutput {
							exit_status: ExitSucceed::Returned,
							output: val.data
						}))
					}
				},
				Err(_) => {
					Some(Err(PrecompileFailure::Error {
						exit_status: ExitError::Other("polkavm failure".into()),
					}))
				},
			}
		} else {
			self.0.execute(handle)
		}
	}

	fn is_precompile(&self, address: H160, remaining_gas: u64) -> IsPrecompileResult {
		let code = pallet_evm::AccountCodes::<T>::get(address);
		if code[0..8] == vm::PREFIX {
			IsPrecompileResult::Answer {
				is_precompile: true,
				extra_cost: 0,
			}
		} else {
			self.0.is_precompile(address, remaining_gas)
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::{ConvertPolkaVmGas, WeightInfo};
	use frame_support::pallet_prelude::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
		type ConvertPolkaVmGas: ConvertPolkaVmGas;
		type WeightInfo: WeightInfo;
	}

	impl<T: Config> Get<u64> for Pallet<T> {
		fn get() -> u64 {
			<ChainId<T>>::get()
		}
	}

	/// The EVM chain ID.
	#[pallet::storage]
	pub type ChainId<T> = StorageValue<_, u64, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub chain_id: u64,
		#[serde(skip)]
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			ChainId::<T>::put(self.chain_id);
		}
	}
}
