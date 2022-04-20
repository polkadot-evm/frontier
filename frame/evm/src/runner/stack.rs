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

//! EVM stack-based runner.

use crate::{
	runner::Runner as RunnerT, AccountCodes, AccountStorages, AddressMapping, BlockHashMapping,
	Config, Error, Event, FeeCalculator, OnChargeEVMTransaction, Pallet,
};
use evm::{
	backend::Backend as BackendT,
	executor::stack::{Accessed, StackExecutor, StackState as StackStateT, StackSubstateMetadata},
	ExitError, ExitReason, Transfer,
};
use fp_evm::{CallInfo, CreateInfo, ExecutionInfo, Log, Vicinity};
use frame_support::{
	ensure,
	traits::{Currency, ExistenceRequirement, Get},
};
use sha3::{Digest, Keccak256};
use sp_core::{H160, H256, U256};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{boxed::Box, collections::btree_set::BTreeSet, marker::PhantomData, mem, vec::Vec};

#[derive(Default)]
pub struct Runner<T: Config> {
	_marker: PhantomData<T>,
}

impl<T: Config> Runner<T> {
	/// Execute an EVM operation.
	pub fn execute<'config, 'precompiles, F, R>(
		source: H160,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		config: &'config evm::Config,
		precompiles: &'precompiles T::PrecompilesType,
		f: F,
	) -> Result<ExecutionInfo<R>, Error<T>>
	where
		F: FnOnce(
			&mut StackExecutor<
				'config,
				'precompiles,
				SubstrateStackState<'_, 'config, T>,
				T::PrecompilesType,
			>,
		) -> (ExitReason, R),
	{
		let base_fee = T::FeeCalculator::min_gas_price();

		let max_fee_per_gas = match (max_fee_per_gas, max_priority_fee_per_gas) {
			(Some(max_fee_per_gas), Some(max_priority_fee_per_gas)) => {
				ensure!(max_fee_per_gas >= base_fee, Error::<T>::GasPriceTooLow);
				ensure!(
					max_fee_per_gas >= max_priority_fee_per_gas,
					Error::<T>::GasPriceTooLow
				);
				max_fee_per_gas
			}
			(Some(max_fee_per_gas), None) => {
				ensure!(max_fee_per_gas >= base_fee, Error::<T>::GasPriceTooLow);
				max_fee_per_gas
			}
			// Gas price check is skipped when performing a gas estimation.
			_ => Default::default(),
		};

		// After eip-1559 we make sure the account can pay both the evm execution and priority fees.
		let total_fee = max_fee_per_gas
			.checked_mul(U256::from(gas_limit))
			.ok_or(Error::<T>::FeeOverflow)?;

		let total_payment = value
			.checked_add(total_fee)
			.ok_or(Error::<T>::PaymentOverflow)?;
		let source_account = Pallet::<T>::account_basic(&source);
		ensure!(
			source_account.balance >= total_payment,
			Error::<T>::BalanceLow
		);

		if let Some(nonce) = nonce {
			ensure!(source_account.nonce == nonce, Error::<T>::InvalidNonce);
		}
		// Deduct fee from the `source` account.
		let fee = T::OnChargeTransaction::withdraw_fee(&source, total_fee)?;

		// Execute the EVM call.
		let vicinity = Vicinity {
			gas_price: base_fee,
			origin: source,
		};

		let metadata = StackSubstateMetadata::new(gas_limit, &config);
		let state = SubstrateStackState::new(&vicinity, metadata);
		let mut executor = StackExecutor::new_with_precompiles(state, config, precompiles);

		let (reason, retv) = f(&mut executor);

		// Post execution.
		let used_gas = U256::from(executor.used_gas());
		let (actual_fee, actual_priority_fee) =
			if let Some(max_priority_fee) = max_priority_fee_per_gas {
				let actual_priority_fee = max_fee_per_gas
					.saturating_sub(base_fee)
					.min(max_priority_fee)
					.checked_mul(U256::from(used_gas))
					.ok_or(Error::<T>::FeeOverflow)?;
				let actual_fee = executor
					.fee(base_fee)
					.checked_add(actual_priority_fee)
					.unwrap_or(U256::max_value());
				(actual_fee, Some(actual_priority_fee))
			} else {
				(executor.fee(base_fee), None)
			};
		log::debug!(
			target: "evm",
			"Execution {:?} [source: {:?}, value: {}, gas_limit: {}, actual_fee: {}]",
			reason,
			source,
			value,
			gas_limit,
			actual_fee
		);
		// The difference between initially withdrawn and the actual cost is refunded.
		//
		// Considered the following request:
		// +-----------+---------+--------------+
		// | Gas_limit | Max_Fee | Max_Priority |
		// +-----------+---------+--------------+
		// |        20 |      10 |            6 |
		// +-----------+---------+--------------+
		//
		// And execution:
		// +----------+----------+
		// | Gas_used | Base_Fee |
		// +----------+----------+
		// |        5 |        2 |
		// +----------+----------+
		//
		// Initially withdrawn 10 * 20 = 200.
		// Actual cost (2 + 6) * 5 = 40.
		// Refunded 200 - 40 = 160.
		// Tip 5 * 6 = 30.
		// Burned 200 - (160 + 30) = 10. Which is equivalent to gas_used * base_fee.
		T::OnChargeTransaction::correct_and_deposit_fee(&source, actual_fee, fee);
		if let Some(actual_priority_fee) = actual_priority_fee {
			T::OnChargeTransaction::pay_priority_fee(actual_priority_fee);
		}

		let state = executor.into_state();

		for address in state.substate.deletes {
			log::debug!(
				target: "evm",
				"Deleting account at {:?}",
				address
			);
			Pallet::<T>::remove_account(&address)
		}

		for log in &state.substate.logs {
			log::trace!(
				target: "evm",
				"Inserting log for {:?}, topics ({}) {:?}, data ({}): {:?}]",
				log.address,
				log.topics.len(),
				log.topics,
				log.data.len(),
				log.data
			);
			Pallet::<T>::deposit_event(Event::<T>::Log(Log {
				address: log.address,
				topics: log.topics.clone(),
				data: log.data.clone(),
			}));
		}

		Ok(ExecutionInfo {
			value: retv,
			exit_reason: reason,
			used_gas,
			logs: state.substate.logs,
		})
	}
}

impl<T: Config> RunnerT<T> for Runner<T> {
	type Error = Error<T>;

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
		config: &evm::Config,
	) -> Result<CallInfo, Self::Error> {
		let precompiles = T::PrecompilesValue::get();
		Self::execute(
			source,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			nonce,
			config,
			&precompiles,
			|executor| executor.transact_call(source, target, value, input, gas_limit, access_list),
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
		config: &evm::Config,
	) -> Result<CreateInfo, Self::Error> {
		let precompiles = T::PrecompilesValue::get();
		Self::execute(
			source,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			nonce,
			config,
			&precompiles,
			|executor| {
				let address = executor.create_address(evm::CreateScheme::Legacy { caller: source });
				let (reason, _) =
					executor.transact_create(source, value, init, gas_limit, access_list);
				(reason, address)
			},
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
		config: &evm::Config,
	) -> Result<CreateInfo, Self::Error> {
		let precompiles = T::PrecompilesValue::get();
		let code_hash = H256::from_slice(Keccak256::digest(&init).as_slice());
		Self::execute(
			source,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			nonce,
			config,
			&precompiles,
			|executor| {
				let address = executor.create_address(evm::CreateScheme::Create2 {
					caller: source,
					code_hash,
					salt,
				});
				let (reason, _) =
					executor.transact_create2(source, value, init, salt, gas_limit, access_list);
				(reason, address)
			},
		)
	}
}

struct SubstrateStackSubstate<'config> {
	metadata: StackSubstateMetadata<'config>,
	deletes: BTreeSet<H160>,
	logs: Vec<Log>,
	parent: Option<Box<SubstrateStackSubstate<'config>>>,
}

impl<'config> SubstrateStackSubstate<'config> {
	pub fn metadata(&self) -> &StackSubstateMetadata<'config> {
		&self.metadata
	}

	pub fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config> {
		&mut self.metadata
	}

	pub fn enter(&mut self, gas_limit: u64, is_static: bool) {
		let mut entering = Self {
			metadata: self.metadata.spit_child(gas_limit, is_static),
			parent: None,
			deletes: BTreeSet::new(),
			logs: Vec::new(),
		};
		mem::swap(&mut entering, self);

		self.parent = Some(Box::new(entering));

		sp_io::storage::start_transaction();
	}

	pub fn exit_commit(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot commit on root substate");
		mem::swap(&mut exited, self);

		self.metadata.swallow_commit(exited.metadata)?;
		self.logs.append(&mut exited.logs);
		self.deletes.append(&mut exited.deletes);

		sp_io::storage::commit_transaction();
		Ok(())
	}

	pub fn exit_revert(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot discard on root substate");
		mem::swap(&mut exited, self);
		self.metadata.swallow_revert(exited.metadata)?;

		sp_io::storage::rollback_transaction();
		Ok(())
	}

	pub fn exit_discard(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot discard on root substate");
		mem::swap(&mut exited, self);
		self.metadata.swallow_discard(exited.metadata)?;

		sp_io::storage::rollback_transaction();
		Ok(())
	}

	pub fn deleted(&self, address: H160) -> bool {
		if self.deletes.contains(&address) {
			return true;
		}

		if let Some(parent) = self.parent.as_ref() {
			return parent.deleted(address);
		}

		false
	}

	pub fn set_deleted(&mut self, address: H160) {
		self.deletes.insert(address);
	}

	pub fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) {
		self.logs.push(Log {
			address,
			topics,
			data,
		});
	}

	fn recursive_is_cold<F: Fn(&Accessed) -> bool>(&self, f: &F) -> bool {
		let local_is_accessed = self.metadata.accessed().as_ref().map(f).unwrap_or(false);
		if local_is_accessed {
			false
		} else {
			self.parent
				.as_ref()
				.map(|p| p.recursive_is_cold(f))
				.unwrap_or(true)
		}
	}
}

/// Substrate backend for EVM.
pub struct SubstrateStackState<'vicinity, 'config, T> {
	vicinity: &'vicinity Vicinity,
	substate: SubstrateStackSubstate<'config>,
	_marker: PhantomData<T>,
}

impl<'vicinity, 'config, T: Config> SubstrateStackState<'vicinity, 'config, T> {
	/// Create a new backend with given vicinity.
	pub fn new(vicinity: &'vicinity Vicinity, metadata: StackSubstateMetadata<'config>) -> Self {
		Self {
			vicinity,
			substate: SubstrateStackSubstate {
				metadata,
				deletes: BTreeSet::new(),
				logs: Vec::new(),
				parent: None,
			},
			_marker: PhantomData,
		}
	}
}

impl<'vicinity, 'config, T: Config> BackendT for SubstrateStackState<'vicinity, 'config, T> {
	fn gas_price(&self) -> U256 {
		self.vicinity.gas_price
	}
	fn origin(&self) -> H160 {
		self.vicinity.origin
	}

	fn block_hash(&self, number: U256) -> H256 {
		if number > U256::from(u32::max_value()) {
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
		let now: u128 = pallet_timestamp::Pallet::<T>::get().unique_saturated_into();
		U256::from(now / 1000)
	}

	fn block_difficulty(&self) -> U256 {
		U256::zero()
	}

	fn block_gas_limit(&self) -> U256 {
		T::BlockGasLimit::get()
	}

	fn chain_id(&self) -> U256 {
		U256::from(T::ChainId::get())
	}

	fn exists(&self, _address: H160) -> bool {
		true
	}

	fn basic(&self, address: H160) -> evm::backend::Basic {
		let account = Pallet::<T>::account_basic(&address);

		evm::backend::Basic {
			balance: account.balance,
			nonce: account.nonce,
		}
	}

	fn code(&self, address: H160) -> Vec<u8> {
		<AccountCodes<T>>::get(&address)
	}

	fn storage(&self, address: H160, index: H256) -> H256 {
		<AccountStorages<T>>::get(address, index)
	}

	fn original_storage(&self, _address: H160, _index: H256) -> Option<H256> {
		None
	}

	fn block_base_fee_per_gas(&self) -> sp_core::U256 {
		T::FeeCalculator::min_gas_price()
	}
}

impl<'vicinity, 'config, T: Config> StackStateT<'config>
	for SubstrateStackState<'vicinity, 'config, T>
{
	fn metadata(&self) -> &StackSubstateMetadata<'config> {
		self.substate.metadata()
	}

	fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config> {
		self.substate.metadata_mut()
	}

	fn enter(&mut self, gas_limit: u64, is_static: bool) {
		self.substate.enter(gas_limit, is_static)
	}

	fn exit_commit(&mut self) -> Result<(), ExitError> {
		self.substate.exit_commit()
	}

	fn exit_revert(&mut self) -> Result<(), ExitError> {
		self.substate.exit_revert()
	}

	fn exit_discard(&mut self) -> Result<(), ExitError> {
		self.substate.exit_discard()
	}

	fn is_empty(&self, address: H160) -> bool {
		Pallet::<T>::is_account_empty(&address)
	}

	fn deleted(&self, address: H160) -> bool {
		self.substate.deleted(address)
	}

	fn inc_nonce(&mut self, address: H160) {
		let account_id = T::AddressMapping::into_account_id(address);
		frame_system::Pallet::<T>::inc_account_nonce(&account_id);
	}

	fn set_storage(&mut self, address: H160, index: H256, value: H256) {
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

	fn reset_storage(&mut self, address: H160) {
		<AccountStorages<T>>::remove_prefix(address, None);
	}

	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) {
		self.substate.log(address, topics, data)
	}

	fn set_deleted(&mut self, address: H160) {
		self.substate.set_deleted(address)
	}

	fn set_code(&mut self, address: H160, code: Vec<u8>) {
		log::debug!(
			target: "evm",
			"Inserting code ({} bytes) at {:?}",
			code.len(),
			address
		);
		Pallet::<T>::create_account(address, code);
	}

	fn transfer(&mut self, transfer: Transfer) -> Result<(), ExitError> {
		let source = T::AddressMapping::into_account_id(transfer.source);
		let target = T::AddressMapping::into_account_id(transfer.target);

		T::Currency::transfer(
			&source,
			&target,
			transfer.value.low_u128().unique_saturated_into(),
			ExistenceRequirement::AllowDeath,
		)
		.map_err(|_| ExitError::OutOfFund)
	}

	fn reset_balance(&mut self, _address: H160) {
		// Do nothing on reset balance in Substrate.
		//
		// This function exists in EVM because a design issue
		// (arguably a bug) in SELFDESTRUCT that can cause total
		// issurance to be reduced. We do not need to replicate this.
	}

	fn touch(&mut self, _address: H160) {
		// Do nothing on touch in Substrate.
		//
		// EVM pallet considers all accounts to exist, and distinguish
		// only empty and non-empty accounts. This avoids many of the
		// subtle issues in EIP-161.
	}

	fn is_cold(&self, address: H160) -> bool {
		self.substate
			.recursive_is_cold(&|a| a.accessed_addresses.contains(&address))
	}

	fn is_storage_cold(&self, address: H160, key: H256) -> bool {
		self.substate
			.recursive_is_cold(&|a: &Accessed| a.accessed_storage.contains(&(address, key)))
	}
}
