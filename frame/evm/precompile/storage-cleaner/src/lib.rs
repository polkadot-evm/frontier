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

//! Storage cleaner precompile. This precompile is used to clean the storage entries of smart contract that
//! has been marked as suicided (self-destructed).

#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

use alloc::vec::Vec;
use core::marker::PhantomData;
use pallet_evm::AddressMapping;
use precompile_utils::{prelude::*, EvmResult};
use sp_core::H160;
use sp_runtime::traits::ConstU32;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const ARRAY_LIMIT: u32 = 1_000;
type GetArrayLimit = ConstU32<ARRAY_LIMIT>;

#[derive(Debug, Clone)]
pub struct StorageCleanerPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> StorageCleanerPrecompile<Runtime>
where
	Runtime: pallet_evm::Config,
{
	/// Clear Storage entries of smart contracts that has been marked as suicided (self-destructed). It takes a list of
	/// addresses and a limit as input. The limit is used to prevent the function from consuming too much gas.
	#[precompile::public("clearSuicidedStorage(address[],uint64)")]
	fn clear_suicided_storage(
		_handle: &mut impl PrecompileHandle,
		addresses: BoundedVec<Address, GetArrayLimit>,
		limit: u64,
	) -> EvmResult {
		if limit == 0 {
			return Err(revert("Limit should be greater than zero"));
		}
		let mut deleted_entries = 0u64;
		let mut deleted_contracts = 0u64;

		let addresses: Vec<_> = addresses.into();
		for Address(address) in addresses {
			if !pallet_evm::Pallet::<Runtime>::is_account_suicided(&address) {
				return Err(revert(alloc::format!("NotSuicided: {}", address)));
			}

			let deleted = pallet_evm::AccountStorages::<Runtime>::drain_prefix(address)
				.take((limit.saturating_sub(deleted_entries)) as usize)
				.count();
			deleted_entries = deleted_entries.saturating_add(deleted as u64);

			// Check if the storage of this contract has been completely removed
			if pallet_evm::AccountStorages::<Runtime>::iter_key_prefix(address)
				.next()
				.is_none()
			{
				Self::clear_suicided_contract(address);
				deleted_contracts = deleted_contracts.saturating_add(1);
			}

			if deleted_entries >= limit {
				break;
			}
		}
		log::info!(target: "evm", "The storage cleaner removed {} entries and {} contracts", deleted_entries, deleted_contracts);

		Ok(())
	}

	/// Clears the storage of a suicided contract.
	///
	/// This function will remove the given address from the list of suicided contracts
	/// and decrement the sufficients of the account associated with the address.
	fn clear_suicided_contract(address: H160) {
		pallet_evm::Suicided::<Runtime>::remove(address);

		let account_id = Runtime::AddressMapping::into_account_id(address);
		let _ = frame_system::Pallet::<Runtime>::dec_sufficients(&account_id);
	}
}
