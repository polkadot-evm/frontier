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

use core::marker::PhantomData;
use pallet_evm::AddressMapping;
use precompile_utils::{prelude::*, EvmResult};
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
	#[precompile::public("clearSuicidedStorage(addresses[])")]
	fn clear_suicided_storage(
		handle: &mut impl PrecompileHandle,
		addresses: BoundedVec<Address, GetArrayLimit>,
	) -> EvmResult {
		let addresses: Vec<_> = addresses.into();

		for address in addresses {
			// Read Suicided storage item
			// Suicided: Blake2128(16) + H160(20)
			handle.record_db_read::<Runtime>(36)?;
			if !pallet_evm::Pallet::<Runtime>::is_account_suicided(&address.0) {
				return Err(revert(format!("NotSuicided: {}", address.0)));
			}

			let mut iter = pallet_evm::Pallet::<Runtime>::iter_account_storages(&address.0).drain();

			loop {
				handle.record_db_read::<Runtime>(116)?;
				// Record the gas cost of deleting the storage item
				handle.record_cost(RuntimeHelper::<Runtime>::db_write_gas_cost())?;

				if iter.next().is_none() {
					handle.refund_external_cost(None, Some(116));
					Self::clear_suicided_contract(address);
					break;
				}
			}
		}

		Ok(())
	}

	fn clear_suicided_contract(address: Address) {
		// Remove the address from the list of suicided contracts
		pallet_evm::Suicided::<Runtime>::remove(&address.0);
		// Decrement the sufficients of the account
		let account_id = Runtime::AddressMapping::into_account_id(address.0);
		let _ = frame_system::Pallet::<Runtime>::dec_sufficients(&account_id);
	}
}
