// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
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

//! # Ethereum pallet
//!
//! The Ethereum pallet works together with EVM pallet to provide full emulation
//! for Ethereum block processing.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use ethereum_types::{Bloom, BloomInput, H160, H256, H64, U256};
use evm::ExitReason;
use fp_consensus::{PostLog, PreLog, FRONTIER_ENGINE_ID};
use fp_evm::CallOrCreateInfo;
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	traits::{EnsureOrigin, Get},
	weights::{Pays, PostDispatchInfo, Weight},
};
use frame_system::{pallet_prelude::OriginFor, WeightInfo};
use pallet_evm::{BlockHashMapping, FeeCalculator, GasWeightMapping, Runner};
use sha3::{Digest, Keccak256};
use sp_runtime::{
	generic::DigestItem,
	traits::{One, Saturating, UniqueSaturatedInto, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransactionBuilder,
	},
	DispatchError, RuntimeDebug,
};
use sp_std::{marker::PhantomData, prelude::*};

pub use ethereum::{
	AccessListItem, BlockV2 as Block, LegacyTransactionMessage, Log, ReceiptV3 as Receipt,
	TransactionAction, TransactionV2 as Transaction,
};
pub use fp_rpc::TransactionStatus;

#[cfg(all(feature = "std", test))]
mod mock;
#[cfg(all(feature = "std", test))]
mod tests;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, scale_info::TypeInfo)]
pub enum RawOrigin {
	EthereumTransaction(H160),
}

pub fn ensure_ethereum_transaction<OuterOrigin>(o: OuterOrigin) -> Result<H160, &'static str>
where
	OuterOrigin: Into<Result<RawOrigin, OuterOrigin>>,
{
	match o.into() {
		Ok(RawOrigin::EthereumTransaction(n)) => Ok(n),
		_ => Err("bad origin: expected to be an Ethereum transaction"),
	}
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
struct TransactionData {
	action: TransactionAction,
	input: Vec<u8>,
	nonce: U256,
	gas_limit: U256,
	gas_price: Option<U256>,
	max_fee_per_gas: Option<U256>,
	max_priority_fee_per_gas: Option<U256>,
	value: U256,
	chain_id: Option<u64>,
	access_list: Vec<(H160, Vec<H256>)>,
}

pub struct EnsureEthereumTransaction;
impl<O: Into<Result<RawOrigin, O>> + From<RawOrigin>> EnsureOrigin<O>
	for EnsureEthereumTransaction
{
	type Success = H160;
	fn try_origin(o: O) -> Result<Self::Success, O> {
		o.into().and_then(|o| match o {
			RawOrigin::EthereumTransaction(id) => Ok(id),
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> O {
		O::from(RawOrigin::EthereumTransaction(Default::default()))
	}
}

impl<T: Config> Call<T>
where
	OriginFor<T>: Into<Result<RawOrigin, OriginFor<T>>>,
{
	pub fn is_self_contained(&self) -> bool {
		match self {
			Call::transact { .. } => true,
			_ => false,
		}
	}

	pub fn check_self_contained(&self) -> Option<Result<H160, TransactionValidityError>> {
		if let Call::transact { transaction } = self {
			let check = || {
				let origin = Pallet::<T>::recover_signer(&transaction).ok_or_else(|| {
					InvalidTransaction::Custom(TransactionValidationError::InvalidSignature as u8)
				})?;

				Ok(origin)
			};

			Some(check())
		} else {
			None
		}
	}

	pub fn pre_dispatch_self_contained(
		&self,
		origin: &H160,
	) -> Option<Result<(), TransactionValidityError>> {
		if let Call::transact { transaction } = self {
			Some(Pallet::<T>::validate_transaction_in_block(
				*origin,
				&transaction,
			))
		} else {
			None
		}
	}

	pub fn validate_self_contained(&self, origin: &H160) -> Option<TransactionValidity> {
		if let Call::transact { transaction } = self {
			Some(Pallet::<T>::validate_transaction_in_pool(
				*origin,
				transaction,
			))
		} else {
			None
		}
	}
}

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ pallet_balances::Config
		+ pallet_timestamp::Config
		+ pallet_evm::Config
	{
		/// The overarching event type.
		type Event: From<Event> + IsType<<Self as frame_system::Config>::Event>;
		/// How Ethereum state root is calculated.
		type StateRoot: Get<H256>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::origin]
	pub type Origin = RawOrigin;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(n: T::BlockNumber) {
			<Pallet<T>>::store_block(
				fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()).is_err(),
				U256::from(UniqueSaturatedInto::<u128>::unique_saturated_into(
					frame_system::Pallet::<T>::block_number(),
				)),
			);
			// move block hash pruning window by one block
			let block_hash_count = T::BlockHashCount::get();
			let to_remove = n
				.saturating_sub(block_hash_count)
				.saturating_sub(One::one());
			// keep genesis hash
			if !to_remove.is_zero() {
				<BlockHash<T>>::remove(U256::from(
					UniqueSaturatedInto::<u32>::unique_saturated_into(to_remove),
				));
			}
		}

		fn on_initialize(_: T::BlockNumber) -> Weight {
			Pending::<T>::kill();
			let mut weight = T::SystemWeightInfo::kill_storage(1);

			// If the digest contain an existing ethereum block(encoded as PreLog), If contains,
			// execute the imported block firstly and disable transact dispatch function.
			if let Ok(log) = fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()) {
				let PreLog::Block(block) = log;

				for transaction in block.transactions {
					let source = Self::recover_signer(&transaction).expect(
						"pre-block transaction signature invalid; the block cannot be built",
					);

					Self::validate_transaction_in_block(source, &transaction).expect(
						"pre-block transaction verification failed; the block cannot be built",
					);
					let r = Self::apply_validated_transaction(source, transaction);
					weight = weight.saturating_add(r.actual_weight.unwrap_or(0 as Weight));
				}
			}
			// Account for `on_finalize` weight:
			//	- read: frame_system::Pallet::<T>::digest()
			//	- read: frame_system::Pallet::<T>::block_number()
			//	- write: <Pallet<T>>::store_block()
			//	- write: <BlockHash<T>>::remove()
			weight.saturating_add(T::DbWeight::get().reads_writes(2, 2))
		}

		fn on_runtime_upgrade() -> Weight {
			frame_support::storage::unhashed::put::<EthereumStorageSchema>(
				&PALLET_ETHEREUM_SCHEMA,
				&EthereumStorageSchema::V3,
			);

			T::DbWeight::get().write
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		OriginFor<T>: Into<Result<RawOrigin, OriginFor<T>>>,
	{
		/// Transact an Ethereum transaction.
		#[pallet::weight(<T as pallet_evm::Config>::GasWeightMapping::gas_to_weight(
			Pallet::<T>::transaction_data(transaction).gas_limit.unique_saturated_into()
		))]
		pub fn transact(
			origin: OriginFor<T>,
			transaction: Transaction,
		) -> DispatchResultWithPostInfo {
			let source = ensure_ethereum_transaction(origin)?;
			// Disable transact functionality if PreLog exist.
			assert!(
				fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()).is_err(),
				"pre log already exists; block is invalid",
			);

			Ok(Self::apply_validated_transaction(source, transaction))
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// An ethereum transaction was successfully executed. [from, to/contract_address, transaction_hash, exit_reason]
		Executed(H160, H160, H256, ExitReason),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Signature is invalid.
		InvalidSignature,
		/// Pre-log is present, therefore transact is not allowed.
		PreLogExists,
	}

	/// Current building block's transactions and receipts.
	#[pallet::storage]
	pub(super) type Pending<T: Config> =
		StorageValue<_, Vec<(Transaction, TransactionStatus, Receipt)>, ValueQuery>;

	/// The current Ethereum block.
	#[pallet::storage]
	pub(super) type CurrentBlock<T: Config> = StorageValue<_, ethereum::BlockV2>;

	/// The current Ethereum receipts.
	#[pallet::storage]
	pub(super) type CurrentReceipts<T: Config> = StorageValue<_, Vec<Receipt>>;

	/// The current transaction statuses.
	#[pallet::storage]
	pub(super) type CurrentTransactionStatuses<T: Config> = StorageValue<_, Vec<TransactionStatus>>;

	// Mapping for block number and hashes.
	#[pallet::storage]
	pub(super) type BlockHash<T: Config> = StorageMap<_, Twox64Concat, U256, H256, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(Default)]
	pub struct GenesisConfig {}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			<Pallet<T>>::store_block(false, U256::zero());
			frame_support::storage::unhashed::put::<EthereumStorageSchema>(
				&PALLET_ETHEREUM_SCHEMA,
				&EthereumStorageSchema::V3,
			);
		}
	}
}

impl<T: Config> Pallet<T> {
	fn transaction_data(transaction: &Transaction) -> TransactionData {
		match transaction {
			Transaction::Legacy(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				value: t.value,
				chain_id: t.signature.chain_id(),
				access_list: Vec::new(),
			},
			Transaction::EIP2930(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				value: t.value,
				chain_id: Some(t.chain_id),
				access_list: t
					.access_list
					.iter()
					.map(|d| (d.address, d.slots.clone()))
					.collect(),
			},
			Transaction::EIP1559(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: None,
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				value: t.value,
				chain_id: Some(t.chain_id),
				access_list: t
					.access_list
					.iter()
					.map(|d| (d.address, d.slots.clone()))
					.collect(),
			},
		}
	}

	fn recover_signer(transaction: &Transaction) -> Option<H160> {
		let mut sig = [0u8; 65];
		let mut msg = [0u8; 32];
		match transaction {
			Transaction::Legacy(t) => {
				sig[0..32].copy_from_slice(&t.signature.r()[..]);
				sig[32..64].copy_from_slice(&t.signature.s()[..]);
				sig[64] = t.signature.standard_v();
				msg.copy_from_slice(
					&ethereum::LegacyTransactionMessage::from(t.clone()).hash()[..],
				);
			}
			Transaction::EIP2930(t) => {
				sig[0..32].copy_from_slice(&t.r[..]);
				sig[32..64].copy_from_slice(&t.s[..]);
				sig[64] = t.odd_y_parity as u8;
				msg.copy_from_slice(
					&ethereum::EIP2930TransactionMessage::from(t.clone()).hash()[..],
				);
			}
			Transaction::EIP1559(t) => {
				sig[0..32].copy_from_slice(&t.r[..]);
				sig[32..64].copy_from_slice(&t.s[..]);
				sig[64] = t.odd_y_parity as u8;
				msg.copy_from_slice(
					&ethereum::EIP1559TransactionMessage::from(t.clone()).hash()[..],
				);
			}
		}
		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg).ok()?;
		Some(H160::from(H256::from_slice(
			Keccak256::digest(&pubkey).as_slice(),
		)))
	}

	fn store_block(post_log: bool, block_number: U256) {
		let mut transactions = Vec::new();
		let mut statuses = Vec::new();
		let mut receipts = Vec::new();
		let mut logs_bloom = Bloom::default();
		let mut cumulative_gas_used = U256::zero();
		for (transaction, status, receipt) in Pending::<T>::get() {
			transactions.push(transaction);
			statuses.push(status);
			receipts.push(receipt.clone());
			let (logs, used_gas) = match receipt {
				Receipt::Legacy(d) | Receipt::EIP2930(d) | Receipt::EIP1559(d) => {
					(d.logs.clone(), d.used_gas)
				}
			};
			cumulative_gas_used = used_gas;
			Self::logs_bloom(logs, &mut logs_bloom);
		}

		let ommers = Vec::<ethereum::Header>::new();
		let receipts_root =
			ethereum::util::ordered_trie_root(receipts.iter().map(|r| rlp::encode(r)));
		let partial_header = ethereum::PartialHeader {
			parent_hash: Self::current_block_hash().unwrap_or_default(),
			beneficiary: pallet_evm::Pallet::<T>::find_author(),
			state_root: T::StateRoot::get(),
			receipts_root,
			logs_bloom,
			difficulty: U256::zero(),
			number: block_number,
			gas_limit: T::BlockGasLimit::get(),
			gas_used: cumulative_gas_used,
			timestamp: UniqueSaturatedInto::<u64>::unique_saturated_into(
				pallet_timestamp::Pallet::<T>::get(),
			),
			extra_data: Vec::new(),
			mix_hash: H256::default(),
			nonce: H64::default(),
		};
		let block = ethereum::Block::new(partial_header, transactions.clone(), ommers);

		CurrentBlock::<T>::put(block.clone());
		CurrentReceipts::<T>::put(receipts.clone());
		CurrentTransactionStatuses::<T>::put(statuses.clone());
		BlockHash::<T>::insert(block_number, block.header.hash());

		if post_log {
			let digest = DigestItem::<T::Hash>::Consensus(
				FRONTIER_ENGINE_ID,
				PostLog::Hashes(fp_consensus::Hashes::from_block(block)).encode(),
			);
			frame_system::Pallet::<T>::deposit_log(digest.into());
		}
	}

	fn logs_bloom(logs: Vec<Log>, bloom: &mut Bloom) {
		for log in logs {
			bloom.accrue(BloomInput::Raw(&log.address[..]));
			for topic in log.topics {
				bloom.accrue(BloomInput::Raw(&topic[..]));
			}
		}
	}

	// Common controls to be performed in the same way by the pool and the
	// State Transition Function (STF).
	// This is the case for all controls except those concerning the nonce.
	fn validate_transaction_common(
		origin: H160,
		transaction_data: &TransactionData,
	) -> Result<(U256, u64), TransactionValidityError> {
		let gas_limit = transaction_data.gas_limit;

		// We must ensure a transaction can pay the cost of its data bytes.
		// If it can't it should not be included in a block.
		let mut gasometer = evm::gasometer::Gasometer::new(
			gas_limit.low_u64(),
			<T as pallet_evm::Config>::config(),
		);
		let transaction_cost = match transaction_data.action {
			TransactionAction::Call(_) => evm::gasometer::call_transaction_cost(
				&transaction_data.input,
				&transaction_data.access_list,
			),
			TransactionAction::Create => evm::gasometer::create_transaction_cost(
				&transaction_data.input,
				&transaction_data.access_list,
			),
		};
		if gasometer.record_transaction(transaction_cost).is_err() {
			return Err(InvalidTransaction::Custom(
				TransactionValidationError::InvalidGasLimit as u8,
			)
			.into());
		}

		if let Some(chain_id) = transaction_data.chain_id {
			if chain_id != T::ChainId::get() {
				return Err(InvalidTransaction::Custom(
					TransactionValidationError::InvalidChainId as u8,
				)
				.into());
			}
		}

		if gas_limit >= T::BlockGasLimit::get() {
			return Err(InvalidTransaction::Custom(
				TransactionValidationError::InvalidGasLimit as u8,
			)
			.into());
		}

		let base_fee = T::FeeCalculator::min_gas_price();
		let mut priority = 0;

		let gas_price = if let Some(gas_price) = transaction_data.gas_price {
			// Legacy and EIP-2930 transactions.
			// Handle priority here. On legacy transaction everything in gas_price except
			// the current base_fee is considered a tip to the miner and thus the priority.
			priority = gas_price.saturating_sub(base_fee).unique_saturated_into();
			gas_price
		} else if let Some(max_fee_per_gas) = transaction_data.max_fee_per_gas {
			// EIP-1559 transactions.
			max_fee_per_gas
		} else {
			return Err(InvalidTransaction::Payment.into());
		};

		if gas_price < base_fee {
			return Err(InvalidTransaction::Payment.into());
		}

		let mut fee = gas_price.saturating_mul(gas_limit);
		if let Some(max_priority_fee_per_gas) = transaction_data.max_priority_fee_per_gas {
			// EIP-1559 transaction priority is determined by `max_priority_fee_per_gas`.
			// If the transaction do not include this optional parameter, priority is now considered zero.
			priority = max_priority_fee_per_gas.unique_saturated_into();
			// Add the priority tip to the payable fee.
			fee = fee.saturating_add(max_priority_fee_per_gas.saturating_mul(gas_limit));
		}

		let account_data = pallet_evm::Pallet::<T>::account_basic(&origin);
		let total_payment = transaction_data.value.saturating_add(fee);
		if account_data.balance < total_payment {
			return Err(InvalidTransaction::Payment.into());
		}

		Ok((account_data.nonce, priority))
	}

	// Controls that must be performed by the pool.
	// The controls common with the State Transition Function (STF) are in
	// the function `validate_transaction_common`.
	fn validate_transaction_in_pool(
		origin: H160,
		transaction: &Transaction,
	) -> TransactionValidity {
		let transaction_data = Pallet::<T>::transaction_data(&transaction);
		let transaction_nonce = transaction_data.nonce;

		let (account_nonce, priority) =
			Self::validate_transaction_common(origin, &transaction_data)?;

		if transaction_nonce < account_nonce {
			return Err(InvalidTransaction::Stale.into());
		}

		// The tag provides and requires must be filled correctly according to the nonce.
		let mut builder = ValidTransactionBuilder::default()
			.and_provides((origin, transaction_nonce))
			.priority(priority);

		// In the context of the pool, a transaction with
		// too high a nonce is still considered valid
		if transaction_nonce > account_nonce {
			if let Some(prev_nonce) = transaction_nonce.checked_sub(1.into()) {
				builder = builder.and_requires((origin, prev_nonce))
			}
		}

		builder.build()
	}

	fn apply_validated_transaction(source: H160, transaction: Transaction) -> PostDispatchInfo {
		let pending = Pending::<T>::get();
		let transaction_hash = transaction.hash();
		let transaction_index = pending.len() as u32;

		let (to, _, info) = Self::execute(source, &transaction, None)
			.expect("transaction is already validated; error indicates that the block is invalid");

		let (reason, status, used_gas, dest) = match info {
			CallOrCreateInfo::Call(info) => (
				info.exit_reason,
				TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to,
					contract_address: None,
					logs: info.logs.clone(),
					logs_bloom: {
						let mut bloom: Bloom = Bloom::default();
						Self::logs_bloom(info.logs, &mut bloom);
						bloom
					},
				},
				info.used_gas,
				to,
			),
			CallOrCreateInfo::Create(info) => (
				info.exit_reason,
				TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to,
					contract_address: Some(info.value),
					logs: info.logs.clone(),
					logs_bloom: {
						let mut bloom: Bloom = Bloom::default();
						Self::logs_bloom(info.logs, &mut bloom);
						bloom
					},
				},
				info.used_gas,
				Some(info.value),
			),
		};

		let receipt = {
			let status_code: u8 = match reason {
				ExitReason::Succeed(_) => 1,
				_ => 0,
			};
			let logs_bloom = status.clone().logs_bloom;
			let logs = status.clone().logs;
			let cumulative_gas_used = if let Some((_, _, receipt)) = pending.last() {
				match receipt {
					Receipt::Legacy(d) | Receipt::EIP2930(d) | Receipt::EIP1559(d) => {
						d.used_gas.saturating_add(used_gas)
					}
				}
			} else {
				used_gas
			};
			match &transaction {
				Transaction::Legacy(_) => Receipt::Legacy(ethereum::EIP658ReceiptData {
					status_code,
					used_gas: cumulative_gas_used,
					logs_bloom,
					logs,
				}),
				Transaction::EIP2930(_) => Receipt::EIP2930(ethereum::EIP2930ReceiptData {
					status_code,
					used_gas: cumulative_gas_used,
					logs_bloom,
					logs,
				}),
				Transaction::EIP1559(_) => Receipt::EIP1559(ethereum::EIP2930ReceiptData {
					status_code,
					used_gas: cumulative_gas_used,
					logs_bloom,
					logs,
				}),
			}
		};

		Pending::<T>::append((transaction, status, receipt));

		Self::deposit_event(Event::Executed(
			source,
			dest.unwrap_or_default(),
			transaction_hash,
			reason,
		));

		PostDispatchInfo {
			actual_weight: Some(T::GasWeightMapping::gas_to_weight(
				used_gas.unique_saturated_into(),
			)),
			pays_fee: Pays::No,
		}
	}

	/// Get the transaction status with given index.
	pub fn current_transaction_statuses() -> Option<Vec<TransactionStatus>> {
		CurrentTransactionStatuses::<T>::get()
	}

	/// Get current block.
	pub fn current_block() -> Option<ethereum::BlockV2> {
		CurrentBlock::<T>::get()
	}

	/// Get current block hash
	pub fn current_block_hash() -> Option<H256> {
		Self::current_block().map(|block| block.header.hash())
	}

	/// Get receipts by number.
	pub fn current_receipts() -> Option<Vec<Receipt>> {
		CurrentReceipts::<T>::get()
	}

	/// Execute an Ethereum transaction.
	pub fn execute(
		from: H160,
		transaction: &Transaction,
		config: Option<evm::Config>,
	) -> Result<(Option<H160>, Option<H160>, CallOrCreateInfo), DispatchError> {
		let (
			input,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			nonce,
			action,
			access_list,
		) = {
			match transaction {
				// max_fee_per_gas and max_priority_fee_per_gas in legacy and 2930 transactions is
				// the provided gas_price.
				Transaction::Legacy(t) => {
					let base_fee = T::FeeCalculator::min_gas_price();
					let priority_fee = t
						.gas_price
						.checked_sub(base_fee)
						.ok_or_else(|| DispatchError::Other("Gas price too low"))?;
					(
						t.input.clone(),
						t.value,
						t.gas_limit,
						Some(base_fee),
						Some(priority_fee),
						Some(t.nonce),
						t.action,
						Vec::new(),
					)
				}
				Transaction::EIP2930(t) => {
					let base_fee = T::FeeCalculator::min_gas_price();
					let priority_fee = t
						.gas_price
						.checked_sub(base_fee)
						.ok_or_else(|| DispatchError::Other("Gas price too low"))?;
					let access_list: Vec<(H160, Vec<H256>)> = t
						.access_list
						.iter()
						.map(|item| (item.address, item.slots.clone()))
						.collect();
					(
						t.input.clone(),
						t.value,
						t.gas_limit,
						Some(base_fee),
						Some(priority_fee),
						Some(t.nonce),
						t.action,
						access_list,
					)
				}
				Transaction::EIP1559(t) => {
					let access_list: Vec<(H160, Vec<H256>)> = t
						.access_list
						.iter()
						.map(|item| (item.address, item.slots.clone()))
						.collect();
					(
						t.input.clone(),
						t.value,
						t.gas_limit,
						Some(t.max_fee_per_gas),
						Some(t.max_priority_fee_per_gas),
						Some(t.nonce),
						t.action,
						access_list,
					)
				}
			}
		};

		match action {
			ethereum::TransactionAction::Call(target) => {
				let res = T::Runner::call(
					from,
					target,
					input.clone(),
					value,
					gas_limit.low_u64(),
					max_fee_per_gas,
					max_priority_fee_per_gas,
					nonce,
					access_list,
					config.as_ref().unwrap_or(T::config()),
				)
				.map_err(Into::into)?;

				Ok((Some(target), None, CallOrCreateInfo::Call(res)))
			}
			ethereum::TransactionAction::Create => {
				let res = T::Runner::create(
					from,
					input.clone(),
					value,
					gas_limit.low_u64(),
					max_fee_per_gas,
					max_priority_fee_per_gas,
					nonce,
					access_list,
					config.as_ref().unwrap_or(T::config()),
				)
				.map_err(Into::into)?;

				Ok((None, Some(res.value), CallOrCreateInfo::Create(res)))
			}
		}
	}

	/// Validate an Ethereum transaction already in block
	///
	/// This function must be called during the pre-dispatch phase
	/// (just before applying the extrinsic).
	pub fn validate_transaction_in_block(
		origin: H160,
		transaction: &Transaction,
	) -> Result<(), TransactionValidityError> {
		let transaction_data = Pallet::<T>::transaction_data(&transaction);
		let transaction_nonce = transaction_data.nonce;
		let (account_nonce, _) = Self::validate_transaction_common(origin, &transaction_data)?;

		// In the context of the block, a transaction with a nonce that is
		// too high should be considered invalid and make the whole block invalid.
		if transaction_nonce > account_nonce {
			Err(TransactionValidityError::Invalid(
				InvalidTransaction::Future,
			))
		} else if transaction_nonce < account_nonce {
			Err(TransactionValidityError::Invalid(InvalidTransaction::Stale))
		} else {
			Ok(())
		}
	}
}

#[derive(Eq, PartialEq, Clone, sp_runtime::RuntimeDebug)]
pub enum ReturnValue {
	Bytes(Vec<u8>),
	Hash(H160),
}

pub struct IntermediateStateRoot;
impl Get<H256> for IntermediateStateRoot {
	fn get() -> H256 {
		H256::decode(&mut &sp_io::storage::root()[..])
			.expect("Node is configured to use the same hash; qed")
	}
}

/// Returns the Ethereum block hash by number.
pub struct EthereumBlockHashMapping<T>(PhantomData<T>);
impl<T: Config> BlockHashMapping for EthereumBlockHashMapping<T> {
	fn block_hash(number: u32) -> H256 {
		BlockHash::<T>::get(U256::from(number))
	}
}

#[repr(u8)]
enum TransactionValidationError {
	#[allow(dead_code)]
	UnknownError,
	InvalidChainId,
	InvalidSignature,
	InvalidGasLimit,
}
