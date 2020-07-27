// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Frontier.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! # Ethereum pallet
//!
//! The Ethereum pallet works together with EVM pallet to provide full emulation
//! for Ethereum block processing.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	decl_module, decl_storage, decl_error, decl_event, ensure,
	traits::Get, traits::FindAuthor
};
use sp_std::prelude::*;
use frame_system::ensure_none;
use ethereum_types::{H160, H64, H256, U256, Bloom};
use sp_runtime::{
	traits::UniqueSaturatedInto,
	transaction_validity::{TransactionValidity, TransactionSource, ValidTransaction}
};
use rlp;
use sha3::{Digest, Keccak256};

pub use frontier_rpc_primitives::TransactionStatus;
pub use ethereum::{Transaction, Log, Block, Receipt, TransactionAction};

#[cfg(all(feature = "std", test))]
mod tests;

#[cfg(all(feature = "std", test))]
mod mock;

/// A type alias for the balance type from this pallet's point of view.
pub type BalanceOf<T> = <T as pallet_balances::Trait>::Balance;

/// Trait for Ethereum pallet.
pub trait Trait: frame_system::Trait<Hash=H256> + pallet_balances::Trait + pallet_timestamp::Trait + pallet_evm::Trait {
	/// The overarching event type.
	type Event: From<Event> + Into<<Self as frame_system::Trait>::Event>;
	/// Find author for Ethereum.
	type FindAuthor: FindAuthor<H160>;
}

decl_storage! {
	trait Store for Module<T: Trait> as Example {
		/// Block hash to block body and block receipt map.
		BlocksAndReceipts: map hasher(blake2_128_concat)
			H256 => Option<(ethereum::Block, Vec<ethereum::Receipt>)>;
		/// Block number to block hash map.
		BlockNumbers: map hasher(blake2_128_concat) T::BlockNumber => H256;
		/// Current building block's transactions and receipts.
		PendingTransactionsAndReceipts: Vec<(ethereum::Transaction, ethereum::Receipt)>;
		/// Transaction hash to transaction status map.
		TransactionStatuses: map hasher(blake2_128_concat) H256 => Option<TransactionStatus>;
		/// Transaction hash to transaction map.
		Transactions: map hasher(blake2_128_concat) H256 => Option<(H256, u32)>;
	}
	add_extra_genesis {
		build(|_config: &GenesisConfig| {
			<Module<T>>::store_block(T::BlockNumber::from(0));
		});
	}
}

decl_event!(
	/// Ethereum pallet events.
	pub enum Event { }
);


decl_error! {
	/// Ethereum pallet errors.
	pub enum Error for Module<T: Trait> {
		/// Transaction signed with wrong chain id
		InvalidChainId,
	}
}

decl_module! {
	/// Ethereum pallet module.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Deposit one of this pallet's events by using the default implementation.
		fn deposit_event() = default;

		/// Transact an Ethereum transaction.
		#[weight = 0]
		fn transact(origin, transaction: ethereum::Transaction) {
			ensure_none(origin)?;

			ensure!(
				transaction.signature.chain_id().unwrap_or_default() == T::ChainId::get(),
				Error::<T>::InvalidChainId
			);
			let mut sig = [0u8; 65];
			let mut msg = [0u8; 32];
			sig[0..32].copy_from_slice(&transaction.signature.r()[..]);
			sig[32..64].copy_from_slice(&transaction.signature.s()[..]);
			sig[64] = transaction.signature.standard_v();
			msg.copy_from_slice(&transaction.message_hash(Some(T::ChainId::get()))[..]);

			let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg)
				.map_err(|_| "Recover public key failed")?;
			let source = H160::from(H256::from_slice(Keccak256::digest(&pubkey).as_slice()));

			Self::execute(source, transaction);
		}

		fn on_finalize(n: T::BlockNumber) {
			<Module<T>>::store_block(n);
		}
	}
}

impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
	type Call = Call<T>;

	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
		ValidTransaction::with_tag_prefix("Ethereum")
			.and_provides(call)
			.build()
	}
}

impl<T: Trait> Module<T> {
	fn store_block(n: T::BlockNumber) {
		let transactions_and_receipts = PendingTransactionsAndReceipts::take();
		let (transactions, receipts): (Vec<_>, Vec<_>) =
			transactions_and_receipts.into_iter().unzip();
		let ommers = Vec::<ethereum::Header>::new();

		let header = ethereum::Header {
			parent_hash: frame_system::Module::<T>::parent_hash(),
			ommers_hash: H256::from_slice(
				Keccak256::digest(&rlp::encode_list(&ommers)[..]).as_slice(),
			), // TODO: check ommers hash.
			beneficiary: <Module<T>>::find_author(),
			state_root: H256::default(), // TODO: figure out if there's better way to get a sort-of-valid state root.
			transactions_root: H256::from_slice(
				Keccak256::digest(&rlp::encode_list(&transactions)[..]).as_slice(),
			), // TODO: check transactions hash.
			receipts_root: H256::from_slice(
				Keccak256::digest(&rlp::encode_list(&receipts)[..]).as_slice(),
			), // TODO: check receipts hash.
			logs_bloom: Bloom::default(), // TODO: gather the logs bloom from receipts.
			difficulty: U256::zero(),
			number: U256::from(
				UniqueSaturatedInto::<u128>::unique_saturated_into(
					frame_system::Module::<T>::block_number()
				)
			),
			gas_limit: U256::zero(), // TODO: set this using Ethereum's gas limit change algorithm.
			gas_used: U256::zero(), // TODO: get this from receipts.
			timestamp: UniqueSaturatedInto::<u64>::unique_saturated_into(
				pallet_timestamp::Module::<T>::get()
			),
			extra_data: H256::default(),
			mix_hash: H256::default(),
			nonce: H64::default(),
		};
		let hash = H256::from_slice(Keccak256::digest(&rlp::encode(&header)).as_slice());

		let block = ethereum::Block {
			header,
			transactions: transactions.clone(),
			ommers,
		};

		for t in &transactions {
			let transaction_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(t)).as_slice()
			);
			if let Some(status) = TransactionStatuses::get(transaction_hash) {
				Transactions::insert(
					transaction_hash,
					(hash, status.transaction_index)
				);
			}
		}

		BlocksAndReceipts::insert(hash, (block, receipts));
		BlockNumbers::<T>::insert(n, hash);
	}

	/// Get the author using the FindAuthor trait.
	pub fn find_author() -> H160 {
		let digest = <frame_system::Module<T>>::digest();
		let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());

		T::FindAuthor::find_author(pre_runtime_digests).unwrap_or_default()
	}

	/// Get the transaction status with given transaction hash.
	pub fn transaction_status(hash: H256) -> Option<TransactionStatus> {
		TransactionStatuses::get(hash)
	}

	/// Get the transaction with given transaction hash.
	pub fn transaction_by_hash(hash: H256) -> Option<(
		ethereum::Transaction,
		ethereum::Block,
		TransactionStatus,
		Vec<ethereum::Receipt>
	)> {
		let (block_hash, transaction_index) = Transactions::get(hash)?;
		let transaction_status = TransactionStatuses::get(hash)?;
		let (block, receipts) = BlocksAndReceipts::get(block_hash)?;
		let transaction = &block.transactions[transaction_index as usize];
		Some((transaction.clone(), block, transaction_status, receipts))
	}

	/// Get transaction by block hash and index.
	pub fn transaction_by_block_hash_and_index(
		hash: H256,
		index: u32
	) -> Option<(
		ethereum::Transaction,
		ethereum::Block,
		TransactionStatus
	)> {
		let (block,_receipt) = BlocksAndReceipts::get(hash)?;
		if index < block.transactions.len() as u32 {
			let transaction = &block.transactions[index as usize];
			let transaction_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(transaction)).as_slice()
			);
			let transaction_status = TransactionStatuses::get(transaction_hash)?;
			Some((transaction.clone(), block, transaction_status))
		} else {
			None
		}
	}

	/// Get transaction by block number and index.
	pub fn transaction_by_block_number_and_index(
		number: T::BlockNumber,
		index: u32
	) -> Option<(
		ethereum::Transaction,
		ethereum::Block,
		TransactionStatus
	)> {
		if <BlockNumbers<T>>::contains_key(number) {
			let hash = <BlockNumbers<T>>::get(number);
			return <Module<T>>::transaction_by_block_hash_and_index(hash, index);
		}
		None
	}

	/// Get block by number.
	pub fn block_by_number(number: T::BlockNumber) -> Option<ethereum::Block> {
		if <BlockNumbers<T>>::contains_key(number) {
			let hash = <BlockNumbers<T>>::get(number);
			if let Some((block, _receipt)) = BlocksAndReceipts::get(hash) {
				return Some(block)
			}
		}
		None
	}

	/// Get block by hash.
	pub fn block_by_hash(hash: H256) -> Option<ethereum::Block> {
		if let Some((block, _receipt)) = BlocksAndReceipts::get(hash) {
			return Some(block)
		}
		None
	}

	/// Get block transaction status of the given block.
	pub fn block_transaction_statuses(
		block: &Block
	) -> Vec<Option<TransactionStatus>> {
		block.transactions.iter().map(|transaction|{
			let transaction_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(transaction)).as_slice()
			);
			<Module<T>>::transaction_status(transaction_hash)
		}).collect()
	}

	fn block_logs(
		block_hash: H256,
		address: Option<H160>,
		topic: Option<Vec<H256>>
	) -> Option<Vec<(
		H160, // address
		Vec<H256>, // topics
		Vec<u8>, // data
		Option<H256>, // block_hash
		Option<U256>, // block_number
		Option<H256>, // transaction_hash
		Option<U256>, // transaction_index
		Option<U256>, // log index in block
		Option<U256>, // log index in transaction
	)>> {
		let mut output = vec![];
		let (block, receipts) = BlocksAndReceipts::get(block_hash)?;
		let mut block_log_index: u32 = 0;
		for (index, receipt) in receipts.iter().enumerate() {
			let logs = receipt.logs.clone();
			let mut transaction_log_index: u32 = 0;
			let transaction = &block.transactions[index as usize];
			let transaction_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(transaction)).as_slice()
			);
			for log in logs {
				let mut add: bool = false;
				if let (Some(address), Some(topics)) = (address.clone(), topic.clone()) {
					if address == log.address && log.topics.starts_with(&topics) {
						add = true;
					}
				} else if let Some(address) = address {
					if address == log.address {
						add = true;
					}
				} else if let Some(topics) = &topic {
					if log.topics.starts_with(&topics) {
						add = true;
					}
				}
				if add {
					output.push((
						log.address.clone(),
						log.topics.clone(),
						log.data.clone(),
						Some(H256::from_slice(
							Keccak256::digest(&rlp::encode(&block.header)).as_slice()
						)),
						Some(block.header.number.clone()),
						Some(transaction_hash),
						Some(U256::from(index)),
						Some(U256::from(block_log_index)),
						Some(U256::from(transaction_log_index))
					));
				}
				transaction_log_index += 1;
				block_log_index += 1;
			}
		}
		Some(output)
	}

	pub fn filtered_logs(
		from_block: Option<u32>,
		to_block: Option<u32>,
		block_hash: Option<H256>,
		address: Option<H160>,
		topic: Option<Vec<H256>>,
	) -> Option<Vec<(
		H160, // address
		Vec<H256>, // topics
		Vec<u8>, // data
		Option<H256>, // block_hash
		Option<U256>, // block_number
		Option<H256>, // transaction_hash
		Option<U256>, // transaction_index
		Option<U256>, // log index in block
		Option<U256>, // log index in transaction
	)>> {
		if let Some(block_hash) = block_hash {
			<Module<T>>::block_logs(
				block_hash,
				address,
				topic
			)
		} else if let (Some(from_block), Some(to_block)) = (from_block, to_block) {
			let mut output = vec![];
			if from_block >= to_block {
				for number in from_block..to_block {
					let block_number = T::BlockNumber::from(number);
					if <BlockNumbers<T>>::contains_key(block_number) {
						let hash = <BlockNumbers<T>>::get(block_number);
						output.extend(<Module<T>>::block_logs(
							hash,
							address.clone(),
							topic.clone()
						).unwrap())
					}
				}
				Some(output)
			} else {
				None
			}
		} else {
			None
		}
	}

	/// Execute an Ethereum transaction, ignoring transaction signatures.
	pub fn execute(source: H160, transaction: ethereum::Transaction) {
		let transaction_hash = H256::from_slice(
			Keccak256::digest(&rlp::encode(&transaction)).as_slice()
		);
		let transaction_index = PendingTransactionsAndReceipts::get().len() as u32;

		let status = match transaction.action {
			ethereum::TransactionAction::Call(target) => {
				pallet_evm::Module::<T>::execute_call(
					source,
					target,
					transaction.input.clone(),
					transaction.value,
					transaction.gas_limit.low_u32(),
					transaction.gas_price,
					Some(transaction.nonce),
					true,
				).unwrap(); // TODO: handle error

				TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to: Some(target),
					contract_address: None,
					logs: Vec::new(), // TODO: feed in logs.
					logs_bloom: Bloom::default(), // TODO: feed in bloom.
				}
			},
			ethereum::TransactionAction::Create => {
				let contract_address = pallet_evm::Module::<T>::execute_create(
					source,
					transaction.input.clone(),
					transaction.value,
					transaction.gas_limit.low_u32(),
					transaction.gas_price,
					Some(transaction.nonce),
					true,
				).unwrap().1; // TODO: handle error

				TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to: None,
					contract_address: Some(contract_address),
					logs: Vec::new(), // TODO: feed in logs.
					logs_bloom: Bloom::default(), // TODO: feed in bloom.
				}
			},
		};

		TransactionStatuses::insert(transaction_hash, status);

		let receipt = ethereum::Receipt {
			state_root: H256::default(), // TODO: should be okay / error status.
			used_gas: U256::default(), // TODO: set this.
			logs_bloom: Bloom::default(), // TODO: set this.
			logs: Vec::new(), // TODO: set this.
		};

		PendingTransactionsAndReceipts::append((transaction, receipt));
	}
}
