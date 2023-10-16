// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
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
//#![deny(unused_crate_dependencies)]

extern crate alloc;

pub const ARRAY_LIMIT: u32 = 1_000;
pub const ENTRY_LIMIT: u32 = 1_000;

use core::marker::PhantomData;
use precompile_utils::{prelude::*, EvmResult};
use pallet_evm::AddressMapping;
use sp_runtime::traits::ConstU32;
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

type GetArrayLimit = ConstU32<ARRAY_LIMIT>;

/// Storage cleaner precompile.
#[derive(Debug, Clone)]
pub struct StorageCleanerPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> StorageCleanerPrecompile<Runtime>
where
	Runtime: pallet_evm::Config,
{
	#[precompile::public("batchSome(address[],uint256[])")]
	fn clear_suicided_storage(
		handle: &mut impl PrecompileHandle,
		addresses: BoundedVec<Address, GetArrayLimit>,
	) -> EvmResult {
		let addresses: Vec<_> = addresses.into();
		let mut deleted_entries = 0;

		// Ensure that all provided addresses are
		'inner: for address in addresses {
			// Read Suicided storage item
			// Suicided: Blake2128(16) + H160(20)
			handle.record_db_read::<Runtime>(36)?;
			if !pallet_evm::Pallet::<Runtime>::is_account_suicided(&address.0) {
				return Err(revert(format!("NotSuicided: {}", address.0)));
			}

			let mut iter = pallet_evm::Pallet::<Runtime>::iter_account_storages(&address.0).drain();
			// Delete a maximum of `ENTRY_LIMIT` entries in AccountStorages prefixed with `address`
			while iter.next().is_some() {
				handle.record_db_read::<Runtime>(116)?;
				// Record the gas cost of deleting the storage item
				handle.record_cost(RuntimeHelper::<Runtime>::db_write_gas_cost())?;

				deleted_entries += 1;
				if deleted_entries >= ENTRY_LIMIT {
					if iter.next().is_none() {
						Self::clear_suicided_contract(address);
					}
					break 'inner;
				}
			}

			// Record the cost of the iteration when `iter.next()` returned `None`
			handle.record_db_read::<Runtime>(116)?;

			// Remove the suicided account
			Self::clear_suicided_contract(address);
		}

		Ok(())
	}

	fn clear_suicided_contract(address: Address) {
		// Remove the suicided account
		pallet_evm::Suicided::<Runtime>::remove(&address.0);
		// Decrement the sufficients of the account
		let account_id = Runtime::AddressMapping::into_account_id(address.0);
		let _ = frame_system::Pallet::<Runtime>::dec_sufficients(&account_id);
	}
}

