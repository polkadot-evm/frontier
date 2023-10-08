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
use precompile_utils::{prelude::*, EvmResult};
use sp_runtime::traits::ConstU32;

type GetArrayLimit = ConstU32<ARRAY_LIMIT>;

/// Storage cleanner precompile.
#[derive(Debug, Clone)]
pub struct StorageCleanerPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> StorageCleanerPrecompile<Runtime>
where
	Runtime: pallet_evm::Config,
{
	#[precompile::public("batchSome(address[])")]
	fn clear_suicided_storage(
		handle: &mut impl PrecompileHandle,
		addresses: BoundedVec<Address, GetArrayLimit>,
	) -> EvmResult {
		let addresses: Vec<_> = addresses.into();

		let mut refounded_gas = 0;

		// Ensure that all provided addresses are
		for address in &addresses {
			// Read Suicided storage item
			// Suicided: Blake2128(16) + H160(20)
			handle.record_db_read::<Runtime>(36)?;
			if !pallet_evm::Pallet::<Runtime>::is_account_suicided(&address.0) {
				return Err(revert("NotSuicided"));
			}

			let mut iter = pallet_evm::Pallet::<Runtime>::iter_account_storages(&address.0).drain();

			'inner: loop {
				// Read AccountStorages item
				// AccountStorages: Blake2128(16) + H160(20) + Blake2128(16) + H256(32) + H256(32)
				if refounded_gas > RuntimeHelper::<Runtime>::db_read_gas_cost() {
					refounded_gas -= RuntimeHelper::<Runtime>::db_read_gas_cost();
				} else {
					handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
				}
				handle.record_external_cost(None, Some(116 as u64))?;

				if iter.next().is_none() {
					// We can't know the exact size of the iterator without consuming it,
					// so we're forced to perform an extra iteration at the end, which is
					// why we refund the cost of this empty iteration.
					handle.refund_external_cost(None, Some(116 as u64));
					refounded_gas += RuntimeHelper::<Runtime>::db_read_gas_cost();

					// TODO remove account
					break 'inner;
				}
			}
		}

		// TODO refund gas

		Ok(())
	}
}
