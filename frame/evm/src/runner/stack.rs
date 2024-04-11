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

//! EVM stack-based runner.

use evm::{
	backend::{OverlayedBackend, OverlayedChangeSet},
	standard::{Etable, EtableResolver, Invoker, TransactArgs, TransactValue},
	ExitError, Log, RuntimeEnvironment,
};
use evm_precompile::StandardPrecompileSet;
// Substrate
use frame_support::{
	traits::{
		tokens::{currency::Currency, ExistenceRequirement},
		Get, Time,
	},
	weights::Weight,
};
use sp_core::{H160, H256, U256};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{
	boxed::Box,
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	mem,
	vec::Vec,
};
// Frontier
use fp_evm::Basic as Account;
// use fp_evm::{
// 	AccessedStorage, CallInfo, CreateInfo, ExecutionInfoV2, IsPrecompileResult, Log, PrecompileSet,
// 	Vicinity, WeightInfo, ACCOUNT_BASIC_PROOF_SIZE, ACCOUNT_CODES_METADATA_PROOF_SIZE,
// 	ACCOUNT_STORAGE_PROOF_SIZE, IS_EMPTY_CHECK_PROOF_SIZE, WRITE_PROOF_SIZE,
// };

use crate::{
	runner::Runner as RunnerT, AccountCodes, AccountCodesMetadata, AccountStorages, AddressMapping,
	BalanceOf, BlockHashMapping, Config, Error, Event, FeeCalculator, OnChargeEVMTransaction,
	OnCreate, Pallet,
};

#[cfg(feature = "forbid-evm-reentrancy")]
environmental::thread_local_impl!(static IN_EVM: environmental::RefCell<bool> = environmental::RefCell::new(false));

#[derive(Default)]
pub struct Runner<T: Config> {
	_marker: PhantomData<T>,
}

impl<T: Config> RunnerT<T> for Runner<T>
where
	BalanceOf<T>: TryFrom<U256> + Into<U256>,
{
	fn validate(
		source: H160,
		target: Option<H160>,
		input: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		evm_config: &evm::standard::Config,
	) -> Result<(), ExitError> {
		Ok(())
	}

	fn call(
		source: H160,
		target: H160,
		input: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &&evm::standard::Config,
	) -> Result<TransactValue, ExitError> {
		let args = TransactArgs::Call {
			caller: source,
			address: target,
			value,
			data: input,
			gas_limit: gas_limit.into(),
			// todo: update this field
			gas_price: max_fee_per_gas.unwrap_or_default(),
			access_list,
		};

		Self::execute(
			&args,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			is_transactional,
			weight_limit,
			proof_size_base_cost,
		)
	}

	fn create(
		source: H160,
		init: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &&evm::standard::Config,
	) -> Result<TransactValue, ExitError> {
		let args = TransactArgs::Create {
			caller: source,
			value,
			init_code: init,
			salt: None,
			gas_limit: gas_limit.into(),
			// todo: update this field
			gas_price: max_fee_per_gas.unwrap_or_default(),
			access_list,
		};

		Self::execute(
			&args,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			is_transactional,
			weight_limit,
			proof_size_base_cost,
		)
	}

	fn create2(
		source: H160,
		init: Vec<u8>,
		salt: H256,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &&evm::standard::Config,
	) -> Result<TransactValue, ExitError> {
		let args = TransactArgs::Create {
			caller: source,
			value,
			init_code: init,
			salt: Some(salt),
			gas_limit: gas_limit.into(),
			// todo: update this field
			gas_price: max_fee_per_gas.unwrap_or_default(),
			access_list,
		};

		Self::execute(
			&args,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			is_transactional,
			weight_limit,
			proof_size_base_cost,
		)
	}
}

impl<T: Config> Runner<T>
where
	BalanceOf<T>: TryFrom<U256> + Into<U256>,
{
	#[allow(clippy::let_and_return)]
	/// Execute an already validated EVM operation.
	fn execute<'config>(
		args: &TransactArgs,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		config: &'config &evm::standard::Config,
		is_transactional: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
	) -> Result<TransactValue, ExitError> {
		#[cfg(feature = "forbid-evm-reentrancy")]
		if IN_EVM.with(|in_evm| in_evm.replace(true)) {
			return Err(RunnerError {
				error: Error::<T>::Reentrancy,
				weight,
			});
		}

		let res = Self::execute_inner(
			&*args,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			is_transactional,
		);

		// Set IN_EVM to false
		// We should make sure that this line is executed whatever the execution path.
		#[cfg(feature = "forbid-evm-reentrancy")]
		let _ = IN_EVM.with(|in_evm| in_evm.take());

		res
	}

	// Execute an already validated EVM operation.
	fn execute_inner<'config, 'precompiles, F, R>(
		args: TransactArgs,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		config: &'config &evm::standard::Config,
		is_transactional: bool,
	) -> Result<TransactValue, ExitError> {
		let precompiles = StandardPrecompileSet::new(&config);
		let gas_etable = Etable::single(evm::standard::eval_gasometer);
		let exec_etable = Etable::runtime();
		let etable = (gas_etable, exec_etable);
		let resolver = EtableResolver::new(&config, &precompiles, &etable);
		let invoker = Invoker::new(&config, &resolver);

		let init_accessed = BTreeSet::new();
		let base_backend = FrontierRuntimeBaseBackend {};
		let mut run_backend = OverlayedBackend::new(base_backend, init_accessed);
		let transact_result = evm::transact(args, 1024, &mut run_backend, &invoker);
		let run_changeset = run_backend.deconstruct().1;

		match transact_result {
			Ok(transact_value) => {
				let OverlayedChangeSet {
					logs,
					balances,
					codes,
					nonces,
					storage_resets,
					storages,
					deletes,
				} = run_changeset;
				base_backend.apply_logs(logs);
				base_backend.apply_balances(balances);
				base_backend.apply_codes(codes);
				base_backend.apply_nonces(nonces);
				base_backend.apply_storage_resets(storage_resets);
				base_backend.apply_storages(storages);
				base_backend.apply_deletes(deletes);

				Ok(transact_value)
			}
			Err(exit_error) => Err(exit_error),
		}
	}
}

struct FrontierRuntimeBaseBackend<T>;

impl<T: Config> RuntimeEnvironment for FrontierRuntimeBaseBackend<T> {
	fn block_hash(&self, number: U256) -> H256 {
		if number > U256::from(u32::MAX) {
			H256::default()
		} else {
			T::BlockHashMapping::block_hash(number.as_u32())
		}
	}

	fn block_number(&self) -> U256 {
		let number: u128 = frame_system::Pallet::<T>::block_number().unique_saturated_into();
		U256::from(number)
	}

	fn block_coinbase(&self) -> H160 {
		Pallet::<T>::find_author()
	}

	fn block_timestamp(&self) -> U256 {
		let now: u128 = T::Timestamp::now().unique_saturated_into();
		U256::from(now / 1000)
	}

	fn block_difficulty(&self) -> U256 {
		U256::zero()
	}

	fn block_randomness(&self) -> Option<H256> {
		None
	}

	fn block_gas_limit(&self) -> U256 {
		T::BlockGasLimit::get()
	}

	fn block_base_fee_per_gas(&self) -> U256 {
		let (base_fee, _) = T::FeeCalculator::min_gas_price();
		base_fee
	}

	fn chain_id(&self) -> U256 {
		U256::from(T::ChainId::get())
	}
}

impl<T: Config> evm::RuntimeBaseBackend for FrontierRuntimeBaseBackend<T> {
	fn balance(&self, address: H160) -> U256 {
		let (account, _) = Pallet::<T>::account_basic(&address);
		account.balance
	}

	fn code_size(&self, address: H160) -> U256 {
		U256::from(<Pallet<T>>::account_code_metadata(address).size)
	}

	fn code_hash(&self, address: H160) -> H256 {
		<Pallet<T>>::account_code_metadata(address).hash
	}

	fn code(&self, address: H160) -> Vec<u8> {
		vec![]
	}

	fn storage(&self, address: H160, index: H256) -> H256 {
		<AccountStorages<T>>::get(address, index)
	}

	fn exists(&self, address: H160) -> bool {
		true
	}

	fn nonce(&self, address: H160) -> U256 {
		let (account, _) = Pallet::<T>::account_basic(&address);
		account.nonce
	}
}

impl<T: Config> FrontierRuntimeBaseBackend<T> {
	fn apply_logs(&self, logs: Vec<Log>) {
		for log in logs {
			Pallet::<T>::deposit_event(Event::<T>::Log {
				log: Log {
					address: log.address,
					topics: log.topics.clone(),
					data: log.data.clone(),
				},
			});
		}
	}

	fn apply_balances(&self, balances: BTreeMap<H160, U256>) {
		todo!()
	}

	fn apply_codes(&self, codes: BTreeMap<H160, Vec<u8>>) {
		for (address, code) in codes {
			Pallet::<T>::create_account(address, code);
		}
	}

	fn apply_nonces(&self, nonces: BTreeMap<H160, U256>) {
		todo!()
	}

	fn apply_storage_resets(&self, addresses: BTreeSet<H160>) {
		for addr in addresses {
			let _ = <AccountStorages<T>>::remove_prefix(addr, None);
		}
	}

	fn apply_storages(&self, storages: BTreeMap<(H160, H256), H256>) {
		for ((address, index), value) in storages {
			// Then we insert or remove the entry based on the value.
			if value == H256::default() {
				log::debug!(
					target: "evm",
					"Removing storage for {:?} [index: {:?}]",
					address,
					index,
				);
				<AccountStorages<T>>::remove(address, index);
			} else {
				log::debug!(
					target: "evm",
					"Updating storage for {:?} [index: {:?}, value: {:?}]",
					address,
					index,
					value,
				);
				<AccountStorages<T>>::insert(address, index, value);
			}
		}
	}

	fn apply_deletes(&self, addresses: BTreeSet<H160>) {}
}
