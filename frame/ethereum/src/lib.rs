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

use frame_support::{
	decl_module, decl_storage, decl_error, decl_event,
	traits::Get, weights::{Pays, PostDispatchInfo, Weight},
	dispatch::DispatchResultWithPostInfo,
};
use sp_std::prelude::*;
use frame_system::ensure_none;
use frame_support::ensure;
use ethereum_types::{H160, H64, H256, U256, Bloom, BloomInput};
use sp_runtime::{
	transaction_validity::{
		TransactionValidity, TransactionSource, InvalidTransaction, ValidTransactionBuilder,
	},
	generic::DigestItem, traits::{Saturating, UniqueSaturatedInto, One, Zero}, DispatchError,
};
use evm::ExitReason;
use fp_evm::CallOrCreateInfo;
use pallet_evm::{Runner, GasWeightMapping, FeeCalculator, BlockHashMapping};
use sha3::{Digest, Keccak256};
use codec::{Encode, Decode};
use fp_consensus::{FRONTIER_ENGINE_ID, PostLog, PreLog};
use fp_storage::PALLET_ETHEREUM_SCHEMA;

pub use fp_rpc::TransactionStatus;
pub use ethereum::{Transaction, Log, Block, Receipt, TransactionAction, TransactionMessage};

#[cfg(all(feature = "std", test))]
mod tests;

#[cfg(all(feature = "std", test))]
mod mock;

#[derive(Eq, PartialEq, Clone, sp_runtime::RuntimeDebug)]
pub enum ReturnValue {
	Bytes(Vec<u8>),
	Hash(H160),
}

/// The schema version for Pallet Ethereum's storage
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord)]
pub enum EthereumStorageSchema {
	Undefined,
	V1,
}

impl Default for EthereumStorageSchema {
	fn default() -> Self {
		Self::Undefined
	}
}

/// A type alias for the balance type from this pallet's point of view.
pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

pub struct IntermediateStateRoot;

impl Get<H256> for IntermediateStateRoot {
	fn get() -> H256 {
		H256::decode(&mut &sp_io::storage::root()[..])
			.expect("Node is configured to use the same hash; qed")
	}
}

/// Configuration trait for Ethereum pallet.
pub trait Config: frame_system::Config<Hash=H256> + pallet_balances::Config + pallet_timestamp::Config + pallet_evm::Config {
	/// The overarching event type.
	type Event: From<Event> + Into<<Self as frame_system::Config>::Event>;
	/// How Ethereum state root is calculated.
	type StateRoot: Get<H256>;
}

decl_storage! {
	trait Store for Module<T: Config> as Ethereum {
		/// Current building block's transactions and receipts.
		Pending: Vec<(ethereum::Transaction, TransactionStatus, ethereum::Receipt)>;

		/// The current Ethereum block.
		CurrentBlock: Option<ethereum::Block>;
		/// The current Ethereum receipts.
		CurrentReceipts: Option<Vec<ethereum::Receipt>>;
		/// The current transaction statuses.
		CurrentTransactionStatuses: Option<Vec<TransactionStatus>>;
		// Mapping for block number and hashes.
		BlockHash: map hasher(twox_64_concat) U256 => H256;
	}
	add_extra_genesis {
		build(|_config: &GenesisConfig| {
			// Calculate the ethereum genesis block
			<Module<T>>::store_block(false, U256::zero());

			// Initialize the storage schema at the well known key.
			frame_support::storage::unhashed::put::<EthereumStorageSchema>(&PALLET_ETHEREUM_SCHEMA, &EthereumStorageSchema::V1);
		});
	}
}

decl_event!(
	/// Ethereum pallet events.
	pub enum Event {
		/// An ethereum transaction was successfully executed. [from, to/contract_address, transaction_hash, exit_reason]
		Executed(H160, H160, H256, ExitReason),
	}
);


decl_error! {
	/// Ethereum pallet errors.
	pub enum Error for Module<T: Config> {
		/// Signature is invalid.
		InvalidSignature,
		/// Pre-log is present, therefore transact is not allowed.
		PreLogExists,
	}
}

decl_module! {
	/// Ethereum pallet module.
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		/// Deposit one of this pallet's events by using the default implementation.
		fn deposit_event() = default;

		/// Transact an Ethereum transaction.
		#[weight = <T as pallet_evm::Config>::GasWeightMapping::gas_to_weight(transaction.gas_limit.unique_saturated_into())]
		fn transact(origin, transaction: ethereum::Transaction) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			Self::do_transact(transaction)
		}

		fn on_finalize(n: T::BlockNumber) {
			<Module<T>>::store_block(
				fp_consensus::find_pre_log(&frame_system::Module::<T>::digest()).is_err(),
				U256::from(
					UniqueSaturatedInto::<u128>::unique_saturated_into(
						frame_system::Module::<T>::block_number()
					)
				),
			);
			// move block hash pruning window by one block
			let block_hash_count = T::BlockHashCount::get();
			let to_remove = n.saturating_sub(block_hash_count).saturating_sub(One::one());
			// keep genesis hash
			if !to_remove.is_zero() {
				BlockHash::remove(U256::from(
					UniqueSaturatedInto::<u32>::unique_saturated_into(to_remove)
				));
			}
		}

		fn on_initialize(_n: T::BlockNumber) -> Weight {
			Pending::kill();

			if let Ok(log) = fp_consensus::find_pre_log(&frame_system::Module::<T>::digest()) {
				let PreLog::Block(block) = log;

				for transaction in block.transactions {
					Self::do_transact(transaction)
						.expect("pre-block transaction verification failed; the block cannot be built");
				}
			}

			0
		}
	}
}

/// Returns the Ethereum block hash by number.
pub struct EthereumBlockHashMapping;
impl BlockHashMapping for EthereumBlockHashMapping {
	fn block_hash(number: u32) -> H256 {
		BlockHash::get(U256::from(number))
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

impl<T: Config> frame_support::unsigned::ValidateUnsigned for Module<T> {
	type Call = Call<T>;

	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
		if let Call::transact(transaction) = call {
			if let Some(chain_id) = transaction.signature.chain_id() {
				if chain_id != T::ChainId::get() {
					return InvalidTransaction::Custom(TransactionValidationError::InvalidChainId as u8).into();
				}
			}

			let origin = Self::recover_signer(&transaction)
				.ok_or_else(|| InvalidTransaction::Custom(TransactionValidationError::InvalidSignature as u8))?;

			if transaction.gas_limit >= T::BlockGasLimit::get() {
				return InvalidTransaction::Custom(TransactionValidationError::InvalidGasLimit as u8).into();
			}

			let account_data = pallet_evm::Module::<T>::account_basic(&origin);

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
		} else {
			Err(InvalidTransaction::Call.into())
		}
	}
}

impl<T: Config> Module<T> {
	fn recover_signer(transaction: &ethereum::Transaction) -> Option<H160> {
		let mut sig = [0u8; 65];
		let mut msg = [0u8; 32];
		sig[0..32].copy_from_slice(&transaction.signature.r()[..]);
		sig[32..64].copy_from_slice(&transaction.signature.s()[..]);
		sig[64] = transaction.signature.standard_v();
		msg.copy_from_slice(&TransactionMessage::from(transaction.clone()).hash()[..]);

		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg).ok()?;
		Some(H160::from(H256::from_slice(Keccak256::digest(&pubkey).as_slice())))
	}

	fn store_block(post_log: bool, block_number: U256) {
		let mut transactions = Vec::new();
		let mut statuses = Vec::new();
		let mut receipts = Vec::new();
		let mut logs_bloom = Bloom::default();
		for (transaction, status, receipt) in Pending::get() {
			transactions.push(transaction);
			statuses.push(status);
			receipts.push(receipt.clone());
			Self::logs_bloom(
				receipt.logs.clone(),
				&mut logs_bloom
			);
		}

		let ommers = Vec::<ethereum::Header>::new();
		let receipts_root = ethereum::util::ordered_trie_root(
			receipts.iter().map(|r| rlp::encode(r))
		);
		let partial_header = ethereum::PartialHeader {
			parent_hash: Self::current_block_hash().unwrap_or_default(),
			beneficiary: pallet_evm::Module::<T>::find_author(),
			state_root: T::StateRoot::get(),
			receipts_root,
			logs_bloom,
			difficulty: U256::zero(),
			number: block_number,
			gas_limit: T::BlockGasLimit::get(),
			gas_used: receipts.clone().into_iter().fold(U256::zero(), |acc, r| acc + r.used_gas),
			timestamp: UniqueSaturatedInto::<u64>::unique_saturated_into(
				pallet_timestamp::Module::<T>::get()
			),
			extra_data: Vec::new(),
			mix_hash: H256::default(),
			nonce: H64::default(),
		};
		let block = ethereum::Block::new(partial_header, transactions.clone(), ommers);

		CurrentBlock::put(block.clone());
		CurrentReceipts::put(receipts.clone());
		CurrentTransactionStatuses::put(statuses.clone());
		BlockHash::insert(block_number, block.header.hash());

		if post_log {
			let digest = DigestItem::<T::Hash>::Consensus(
				FRONTIER_ENGINE_ID,
				PostLog::Hashes(fp_consensus::Hashes::from_block(block)).encode(),
			);
			frame_system::Module::<T>::deposit_log(digest.into());
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

	fn do_transact(transaction: ethereum::Transaction) -> DispatchResultWithPostInfo {
		ensure!(
			fp_consensus::find_pre_log(&frame_system::Module::<T>::digest()).is_err(),
			Error::<T>::PreLogExists,
		);

		let source = Self::recover_signer(&transaction)
			.ok_or_else(|| Error::<T>::InvalidSignature)?;

		let transaction_hash = H256::from_slice(
			Keccak256::digest(&rlp::encode(&transaction)).as_slice()
		);
		let transaction_index = Pending::get().len() as u32;

		let (to, contract_address, info) = Self::execute(
			source,
			transaction.input.clone(),
			transaction.value,
			transaction.gas_limit,
			Some(transaction.gas_price),
			Some(transaction.nonce),
			transaction.action,
			None,
		)?;

		let (reason, status, used_gas) = match info {
			CallOrCreateInfo::Call(info) => {
				(info.exit_reason, TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to,
					contract_address: None,
					logs: info.logs.clone(),
					logs_bloom: {
						let mut bloom: Bloom = Bloom::default();
						Self::logs_bloom(
							info.logs,
							&mut bloom
						);
						bloom
					},
				}, info.used_gas)
			},
			CallOrCreateInfo::Create(info) => {
				(info.exit_reason, TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to,
					contract_address: Some(info.value),
					logs: info.logs.clone(),
					logs_bloom: {
						let mut bloom: Bloom = Bloom::default();
						Self::logs_bloom(
							info.logs,
							&mut bloom
						);
						bloom
					},
				}, info.used_gas)
			},
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

		Pending::append((transaction, status, receipt));

		Self::deposit_event(Event::Executed(source, contract_address.unwrap_or_default(), transaction_hash, reason));
		Ok(PostDispatchInfo {
			actual_weight: Some(T::GasWeightMapping::gas_to_weight(used_gas.unique_saturated_into())),
			pays_fee: Pays::No,
		}).into()
	}

	/// Get the transaction status with given index.
	pub fn current_transaction_statuses() -> Option<Vec<TransactionStatus>> {
		CurrentTransactionStatuses::get()
	}

	/// Get current block.
	pub fn current_block() -> Option<ethereum::Block> {
		CurrentBlock::get()
	}

	/// Get current block hash
	pub fn current_block_hash() -> Option<H256> {
		Self::current_block().map(|block| block.header.hash())
	}

	/// Get receipts by number.
	pub fn current_receipts() -> Option<Vec<ethereum::Receipt>> {
		CurrentReceipts::get()
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
				).map_err(Into::into)?;

				Ok((Some(target), None, CallOrCreateInfo::Call(res)))
			},
			ethereum::TransactionAction::Create => {
				let res = T::Runner::create(
					from,
					input.clone(),
					value,
					gas_limit.low_u64(),
					gas_price,
					nonce,
					config.as_ref().unwrap_or(T::config()),
				).map_err(Into::into)?;

				Ok((None, Some(res.value), CallOrCreateInfo::Create(res)))
			},
		}
	}
}
