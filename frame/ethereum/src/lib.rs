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
use fp_storage::PALLET_ETHEREUM_SCHEMA;
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	ensure,
	traits::{EnsureOrigin, Get},
	weights::{Pays, PostDispatchInfo, Weight},
};
use frame_system::pallet_prelude::OriginFor;
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
	BlockV0 as Block, LegacyTransactionMessage, Log, Receipt, TransactionAction,
	TransactionV0 as Transaction,
};
pub use fp_rpc::TransactionStatus;

#[cfg(all(feature = "std", test))]
mod mock;
#[cfg(all(feature = "std", test))]
mod tests;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
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
			Call::transact(_) => true,
			_ => false,
		}
	}

	pub fn check_self_contained(&self) -> Option<Result<H160, TransactionValidityError>> {
		if let Call::transact(transaction) = self {
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

	pub fn validate_self_contained(&self, origin: &H160) -> Option<TransactionValidity> {
		if let Call::transact(transaction) = self {
			let validate = || {
				// We must ensure a transaction can pay the cost of its data bytes.
				// If it can't it should not be included in a block.
				let mut gasometer = evm::gasometer::Gasometer::new(
					transaction.gas_limit.low_u64(),
					<T as pallet_evm::Config>::config(),
				);
				let transaction_cost = match transaction.action {
					TransactionAction::Call(_) => {
						evm::gasometer::call_transaction_cost(&transaction.input)
					}
					TransactionAction::Create => {
						evm::gasometer::create_transaction_cost(&transaction.input)
					}
				};
				if gasometer.record_transaction(transaction_cost).is_err() {
					return InvalidTransaction::Custom(
						TransactionValidationError::InvalidGasLimit as u8,
					)
					.into();
				}

				if let Some(chain_id) = transaction.signature.chain_id() {
					if chain_id != T::ChainId::get() {
						return InvalidTransaction::Custom(
							TransactionValidationError::InvalidChainId as u8,
						)
						.into();
					}
				}

				if transaction.gas_limit >= T::BlockGasLimit::get() {
					return InvalidTransaction::Custom(
						TransactionValidationError::InvalidGasLimit as u8,
					)
					.into();
				}

				let account_data = pallet_evm::Pallet::<T>::account_basic(&origin);

				if transaction.nonce < account_data.nonce {
					return InvalidTransaction::Stale.into();
				}

				let fee = transaction.gas_price.saturating_mul(transaction.gas_limit);
				let total_payment = transaction.value.saturating_add(fee);
				if account_data.balance < total_payment {
					return InvalidTransaction::Payment.into();
				}

				let min_gas_price = T::FeeCalculator::min_gas_price();

				if transaction.gas_price < min_gas_price {
					return InvalidTransaction::Payment.into();
				}

				let mut builder = ValidTransactionBuilder::default()
					.and_provides((origin, transaction.nonce))
					.priority(transaction.gas_price.unique_saturated_into());

				if transaction.nonce > account_data.nonce {
					if let Some(prev_nonce) = transaction.nonce.checked_sub(1.into()) {
						builder = builder.and_requires((origin, prev_nonce))
					}
				}

				builder.build()
			};

			Some(validate())
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

			if let Ok(log) = fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()) {
				let PreLog::Block(block) = log;

				for transaction in block.transactions {
					let source = Self::recover_signer(&transaction).expect(
						"pre-block transaction signature invalid; the block cannot be built",
					);

					Self::do_transact(source, transaction).expect(
						"pre-block transaction verification failed; the block cannot be built",
					);
				}
			}

			0
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		OriginFor<T>: Into<Result<RawOrigin, OriginFor<T>>>,
	{
		/// Transact an Ethereum transaction.
		#[pallet::weight(<T as pallet_evm::Config>::GasWeightMapping::gas_to_weight(transaction.gas_limit.unique_saturated_into()))]
		pub fn transact(
			origin: OriginFor<T>,
			transaction: Transaction,
		) -> DispatchResultWithPostInfo {
			let source = ensure_ethereum_transaction(origin)?;

			Self::do_transact(source, transaction)
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
		StorageValue<_, Vec<(Transaction, TransactionStatus, ethereum::Receipt)>, ValueQuery>;

	/// The current Ethereum block.
	#[pallet::storage]
	pub(super) type CurrentBlock<T: Config> = StorageValue<_, ethereum::BlockV0>;

	/// The current Ethereum receipts.
	#[pallet::storage]
	pub(super) type CurrentReceipts<T: Config> = StorageValue<_, Vec<ethereum::Receipt>>;

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
				&EthereumStorageSchema::V1,
			);
		}
	}
}

impl<T: Config> Pallet<T> {
	fn recover_signer(transaction: &Transaction) -> Option<H160> {
		let mut sig = [0u8; 65];
		let mut msg = [0u8; 32];
		sig[0..32].copy_from_slice(&transaction.signature.r()[..]);
		sig[32..64].copy_from_slice(&transaction.signature.s()[..]);
		sig[64] = transaction.signature.standard_v();
		msg.copy_from_slice(&LegacyTransactionMessage::from(transaction.clone()).hash()[..]);

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
		for (transaction, status, receipt) in Pending::<T>::get() {
			transactions.push(transaction);
			statuses.push(status);
			receipts.push(receipt.clone());
			Self::logs_bloom(receipt.logs.clone(), &mut logs_bloom);
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
			gas_used: receipts
				.clone()
				.into_iter()
				.fold(U256::zero(), |acc, r| acc + r.used_gas),
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

	fn do_transact(source: H160, transaction: Transaction) -> DispatchResultWithPostInfo {
		ensure!(
			fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()).is_err(),
			Error::<T>::PreLogExists,
		);

		let transaction_hash =
			H256::from_slice(Keccak256::digest(&rlp::encode(&transaction)).as_slice());
		let transaction_index = Pending::<T>::get().len() as u32;

		let (to, _, info) = Self::execute(
			source,
			transaction.input.clone(),
			transaction.value,
			transaction.gas_limit,
			Some(transaction.gas_price),
			Some(transaction.nonce),
			transaction.action,
			None,
		)?;

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

		let receipt = ethereum::Receipt {
			state_root: match reason {
				ExitReason::Succeed(_) => H256::from_low_u64_be(1),
				ExitReason::Error(_) => H256::from_low_u64_le(0),
				ExitReason::Revert(_) => H256::from_low_u64_le(0),
				ExitReason::Fatal(_) => H256::from_low_u64_le(0),
			},
			used_gas,
			logs_bloom: status.clone().logs_bloom,
			logs: status.clone().logs,
		};

		Pending::<T>::append((transaction, status, receipt));

		Self::deposit_event(Event::Executed(
			source,
			dest.unwrap_or_default(),
			transaction_hash,
			reason,
		));
		Ok(PostDispatchInfo {
			actual_weight: Some(T::GasWeightMapping::gas_to_weight(
				used_gas.unique_saturated_into(),
			)),
			pays_fee: Pays::No,
		})
		.into()
	}

	/// Get the transaction status with given index.
	pub fn current_transaction_statuses() -> Option<Vec<TransactionStatus>> {
		CurrentTransactionStatuses::<T>::get()
	}

	/// Get current block.
	pub fn current_block() -> Option<ethereum::BlockV0> {
		CurrentBlock::<T>::get()
	}

	/// Get current block hash
	pub fn current_block_hash() -> Option<H256> {
		Self::current_block().map(|block| block.header.hash())
	}

	/// Get receipts by number.
	pub fn current_receipts() -> Option<Vec<ethereum::Receipt>> {
		CurrentReceipts::<T>::get()
	}

	/// Execute an Ethereum transaction.
	pub fn execute(
		from: H160,
		input: Vec<u8>,
		value: U256,
		gas_limit: U256,
		gas_price: Option<U256>,
		nonce: Option<U256>,
		action: TransactionAction,
		config: Option<evm::Config>,
	) -> Result<(Option<H160>, Option<H160>, CallOrCreateInfo), DispatchError> {
		match action {
			ethereum::TransactionAction::Call(target) => {
				let res = T::Runner::call(
					from,
					target,
					input.clone(),
					value,
					gas_limit.low_u64(),
					gas_price,
					nonce,
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
					gas_price,
					nonce,
					config.as_ref().unwrap_or(T::config()),
				)
				.map_err(Into::into)?;

				Ok((None, Some(res.value), CallOrCreateInfo::Create(res)))
			}
		}
	}
}

#[derive(Eq, PartialEq, Clone, sp_runtime::RuntimeDebug)]
pub enum ReturnValue {
	Bytes(Vec<u8>),
	Hash(H160),
}

/// The schema version for Pallet Ethereum's storage
#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord)]
pub enum EthereumStorageSchema {
	Undefined,
	V1,
}

impl Default for EthereumStorageSchema {
	fn default() -> Self {
		Self::Undefined
	}
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
