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

use core::marker::PhantomData;
use fp_evm::{
	ExitError, ExitRevert, ExitSucceed, IsPrecompileResult, PrecompileFailure, PrecompileHandle,
	PrecompileOutput, PrecompileSet,
};
use sp_core::{H160, H256};

pub use self::{pallet::*, weights::WeightInfo};

pub trait CreateAddressScheme<AccountId> {
	fn create_address_scheme(caller: AccountId, code: &[u8], salt: H256) -> H160;
}

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
	fn execute(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<Result<PrecompileOutput, PrecompileFailure>> {
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
							output: val.data,
						}))
					} else {
						Some(Ok(PrecompileOutput {
							exit_status: ExitSucceed::Returned,
							output: val.data,
						}))
					}
				}
				Err(_) => Some(Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("polkavm failure".into()),
				})),
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
	use super::{ConvertPolkaVmGas, CreateAddressScheme, WeightInfo};
	use fp_evm::AccountProvider;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use pallet_evm::{
		AccountCodes, AccountCodesMetadata, AddressMapping, CodeMetadata, Config as EConfig,
	};
	use sp_core::H256;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
		type CreateAddressScheme: CreateAddressScheme<<Self as frame_system::Config>::AccountId>;
		type ConvertPolkaVmGas: ConvertPolkaVmGas;
		type MaxCodeSize: Get<u32>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Maximum code length exceeded.
		MaxCodeSizeExceeded,
		/// Not deploying PolkaVM contract.
		NotPolkaVmContract,
		/// Contract already exist in state.
		AlreadyExist,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Deploy a new PolkaVM contract into the Frontier state.
		///
		/// A PolkaVM contract is simply a contract in the Frontier state prefixed
		/// by `0xef polkavm`. EIP-3541 ensures that no EVM contract will starts with
		/// the prefix.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::create_polkavm(code.len() as u32))]
		pub fn create_polkavm(origin: OriginFor<T>, code: Vec<u8>, salt: H256) -> DispatchResult {
			if code.len() as u32 >= <T as Config>::MaxCodeSize::get() {
				return Err(Error::<T>::MaxCodeSizeExceeded.into());
			}

			if code[0..8] != crate::vm::PREFIX {
				return Err(Error::<T>::NotPolkaVmContract.into());
			}

			let caller = ensure_signed(origin)?;
			let address =
				<T as Config>::CreateAddressScheme::create_address_scheme(caller, &code[..], salt);

			if <AccountCodes<T>>::contains_key(address) {
				return Err(Error::<T>::AlreadyExist.into());
			}

			let account_id = <T as EConfig>::AddressMapping::into_account_id(address);
			<T as EConfig>::AccountProvider::create_account(&account_id);

			let meta = CodeMetadata::from_code(&code);
			<AccountCodesMetadata<T>>::insert(address, meta);
			<AccountCodes<T>>::insert(address, code);

			Ok(())
		}
	}
}
