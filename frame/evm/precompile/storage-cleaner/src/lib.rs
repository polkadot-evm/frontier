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

//! Storage cleaner precompile. This precompile is used to clean the storage entries of smart contract that
//! has been marked as suicided (self-destructed).

use core::marker::PhantomData;
use fp_evm::{PrecompileFailure, ACCOUNT_BASIC_PROOF_SIZE, ACCOUNT_STORAGE_PROOF_SIZE};
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
// Storage key for suicided contracts: Blake2_128(16) + Key (H160(20))
pub const SUICIDED_STORAGE_KEY: u64 = 36;

#[derive(Debug, Clone)]
pub struct StorageCleanerPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> StorageCleanerPrecompile<Runtime>
where
	Runtime: pallet_evm::Config,
{
	/// Clear Storage entries of smart contracts that has been marked as suicided (self-destructed). It takes a list of
	/// addresses and a limit as input. The limit is used to prevent the function from consuming too much gas. The
	/// maximum number of storage entries that can be removed is limit - 1.
	#[precompile::public("clearSuicidedStorage(address[],uint64)")]
	fn clear_suicided_storage(
		handle: &mut impl PrecompileHandle,
		addresses: BoundedVec<Address, GetArrayLimit>,
		limit: u64,
	) -> EvmResult {
		let addresses: Vec<_> = addresses.into();
		let nb_addresses = addresses.len() as u64;
		if limit == 0 {
			return Err(revert("Limit should be greater than zero"));
		}

		Self::record_max_cost(handle, nb_addresses, limit)?;
		let result = Self::clear_suicided_storage_inner(addresses, limit - 1)?;
		Self::refund_cost(handle, result, nb_addresses, limit);

		Ok(())
	}

	/// This function iterates over the addresses, checks if each address is marked as suicided, and then deletes the storage
	/// entries associated with that address. If there are no remaining entries, we clear the suicided contract by calling the
	/// `clear_suicided_contract` function.
	fn clear_suicided_storage_inner(
		addresses: Vec<Address>,
		limit: u64,
	) -> Result<RemovalResult, PrecompileFailure> {
		let mut deleted_entries = 0u64;
		let mut deleted_contracts = 0u64;

		for Address(address) in addresses {
			if !pallet_evm::Pallet::<Runtime>::is_account_suicided(&address) {
				return Err(revert(format!("NotSuicided: {}", address)));
			}

			let deleted = pallet_evm::AccountStorages::<Runtime>::drain_prefix(address)
				.take((limit.saturating_sub(deleted_entries)) as usize)
				.count();
			deleted_entries = deleted_entries.saturating_add(deleted as u64);

			// Check if the storage of this contract has been completly removed
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

		Ok(RemovalResult {
			deleted_entries,
			deleted_contracts,
		})
	}

	/// Record the maximum cost (Worst case Scenario) of the clear_suicided_storage function.
	fn record_max_cost(
		handle: &mut impl PrecompileHandle,
		nb_addresses: u64,
		limit: u64,
	) -> EvmResult {
		let read_cost = RuntimeHelper::<Runtime>::db_read_gas_cost();
		let write_cost = RuntimeHelper::<Runtime>::db_write_gas_cost();
		let ref_time = 0u64
			// EVM:: Suicided (reads = nb_addresses)
			.saturating_add(read_cost.saturating_mul(nb_addresses))
			// EVM:: Suicided (writes = nb_addresses)
			.saturating_add(write_cost.saturating_mul(nb_addresses))
			// System: AccountInfo (reads = nb_addresses) for decrementing sufficients
			.saturating_add(read_cost.saturating_mul(nb_addresses))
			// System: AccountInfo (writes = nb_addresses) for decrementing sufficients
			.saturating_add(write_cost.saturating_mul(nb_addresses))
			// EVM: AccountStorage (reads = limit)
			.saturating_add(read_cost.saturating_mul(limit))
			// EVM: AccountStorage (writes = limit)
			.saturating_add(write_cost.saturating_mul(limit));

		let proof_size = 0u64
			// Proof: EVM::Suicided (SUICIDED_STORAGE_KEY) * nb_addresses
			.saturating_add(SUICIDED_STORAGE_KEY.saturating_mul(nb_addresses))
			// Proof: EVM::AccountStorage (ACCOUNT_BASIC_PROOF_SIZE) * limit
			.saturating_add(ACCOUNT_STORAGE_PROOF_SIZE.saturating_mul(limit))
			// Proof: System::AccountInfo (ACCOUNT_BASIC_PROOF_SIZE) * nb_addresses
			.saturating_add(ACCOUNT_BASIC_PROOF_SIZE.saturating_mul(nb_addresses));

		handle.record_external_cost(Some(ref_time), Some(proof_size), None)?;
		Ok(())
	}

	/// Refund the additional cost recorded for the clear_suicided_storage function.
	fn refund_cost(
		handle: &mut impl PrecompileHandle,
		result: RemovalResult,
		nb_addresses: u64,
		limit: u64,
	) {
		let read_cost = RuntimeHelper::<Runtime>::db_read_gas_cost();
		let write_cost = RuntimeHelper::<Runtime>::db_write_gas_cost();

		let extra_entries = limit.saturating_sub(result.deleted_entries);
		let extra_contracts = nb_addresses.saturating_sub(result.deleted_contracts);

		let mut ref_time = 0u64;
		let mut proof_size = 0u64;

		// Refund the cost of the remaining entries
		if extra_entries > 0 {
			ref_time = ref_time
				// EVM:: AccountStorage (reads = extra_entries)
				.saturating_add(read_cost.saturating_mul(extra_entries))
				// EVM:: AccountStorage (writes = extra_entries)
				.saturating_add(write_cost.saturating_mul(extra_entries));
			proof_size = proof_size
				// Proof: EVM::AccountStorage (ACCOUNT_BASIC_PROOF_SIZE) * extra_entries
				.saturating_add(ACCOUNT_STORAGE_PROOF_SIZE.saturating_mul(extra_entries));
		}

		// Refund the cost of the remaining contracts
		if extra_contracts > 0 {
			ref_time = ref_time
				// EVM:: Suicided (reads = extra_contracts)
				.saturating_add(read_cost.saturating_mul(extra_contracts))
				// EVM:: Suicided (writes = extra_contracts)
				.saturating_add(write_cost.saturating_mul(extra_contracts))
				// System: AccountInfo (reads = extra_contracts) for decrementing sufficients
				.saturating_add(read_cost.saturating_mul(extra_contracts))
				// System: AccountInfo (writes = extra_contracts) for decrementing sufficients
				.saturating_add(write_cost.saturating_mul(extra_contracts));
			proof_size = proof_size
				// Proof: EVM::Suicided (SUICIDED_STORAGE_KEY) * extra_contracts
				.saturating_add(SUICIDED_STORAGE_KEY.saturating_mul(extra_contracts))
				// Proof: System::AccountInfo (ACCOUNT_BASIC_PROOF_SIZE) * extra_contracts
				.saturating_add(ACCOUNT_BASIC_PROOF_SIZE.saturating_mul(extra_contracts));
		}

		handle.refund_external_cost(Some(ref_time), Some(proof_size));
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

struct RemovalResult {
	pub deleted_entries: u64,
	pub deleted_contracts: u64,
}
