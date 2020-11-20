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
	decl_module, decl_storage, decl_error, decl_event,
	traits::Get, traits::FindAuthor,
};
use sp_std::prelude::*;
use frame_system::ensure_none;
use ethereum_types::{H160, H64, H256, U256, Bloom};
use sp_runtime::{
	transaction_validity::{
		TransactionValidity, TransactionSource, InvalidTransaction, ValidTransactionBuilder,
	},
	generic::DigestItem, traits::UniqueSaturatedInto, DispatchError,
};
use evm::ExitReason;
use fp_evm::CallOrCreateInfo;
use pallet_evm::Runner;
use sha3::{Digest, Keccak256};
use codec::Encode;
use fp_consensus::{FRONTIER_ENGINE_ID, ConsensusLog};

pub use frontier_rpc_primitives::TransactionStatus;
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
	trait Store for Module<T: Trait> as Ethereum {
		/// Current building block's transactions and receipts.
		Pending: Vec<(ethereum::Transaction, TransactionStatus, ethereum::Receipt)>;

		/// The current Ethereum block.
		CurrentBlock: Option<ethereum::Block>;
		/// The current Ethereum receipts.
		CurrentReceipts: Option<Vec<ethereum::Receipt>>;
		/// The current transaction statuses.
		CurrentTransactionStatuses: Option<Vec<TransactionStatus>>;
	}
	add_extra_genesis {
		build(|_config: &GenesisConfig| {
			<Module<T>>::store_block();
		});
	}
}

decl_event!(
	/// Ethereum pallet events.
	pub enum Event {
		/// An ethereum transaction was successfully executed. [from, transaction_hash]
		Executed(H160, H256, ExitReason),
	}
);


decl_error! {
	/// Ethereum pallet errors.
	pub enum Error for Module<T: Trait> {
		/// Signature is invalid.
		InvalidSignature,
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

			let source = Self::recover_signer(&transaction)
				.ok_or_else(|| Error::<T>::InvalidSignature)?;

			let transaction_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(&transaction)).as_slice()
			);
			let transaction_index = Pending::get().len() as u32;

			let (to, info) = Self::execute(
				source,
				transaction.input.clone(),
				transaction.value,
				transaction.gas_limit,
				Some(transaction.gas_price),
				Some(transaction.nonce),
				transaction.action,
			)?;

			let (reason, status, used_gas) = match info {
				CallOrCreateInfo::Call(info) => {
					(info.exit_reason, TransactionStatus {
						transaction_hash,
						transaction_index,
						from: source,
						to,
						contract_address: None,
						logs: info.logs,
						logs_bloom: Bloom::default(), // TODO: feed in bloom.
					}, info.used_gas)
				},
				CallOrCreateInfo::Create(info) => {
					(info.exit_reason, TransactionStatus {
						transaction_hash,
						transaction_index,
						from: source,
						to,
						contract_address: Some(info.value),
						logs: info.logs,
						logs_bloom: Bloom::default(), // TODO: feed in bloom.
					}, info.used_gas)
				},
			};

			let receipt = ethereum::Receipt {
				state_root: H256::default(), // TODO: should be okay / error status.
				used_gas,
				logs_bloom: Bloom::default(), // TODO: set this.
				logs: status.clone().logs,
			};

			Pending::append((transaction, status, receipt));

			Self::deposit_event(Event::Executed(source, transaction_hash, reason));
		}

		fn on_finalize(n: T::BlockNumber) {
			<Module<T>>::store_block();
		}

		fn on_initialize(n: T::BlockNumber) -> frame_support::weights::Weight {
			Pending::kill();
			0
		}
	}
}

#[repr(u8)]
enum TransactionValidationError {
	#[allow(dead_code)]
	UnknownError,
	InvalidChainId,
	InvalidSignature,
}

impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
	type Call = Call<T>;

	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
		if let Call::transact(transaction) = call {
			if transaction.signature.chain_id().unwrap_or_default() != T::ChainId::get() {
				return InvalidTransaction::Custom(TransactionValidationError::InvalidChainId as u8).into();
			}

			let origin = Self::recover_signer(&transaction)
				.ok_or_else(|| InvalidTransaction::Custom(TransactionValidationError::InvalidSignature as u8))?;

			let account_data = pallet_evm::Module::<T>::account_basic(&origin);

			if transaction.nonce < account_data.nonce {
				return InvalidTransaction::Stale.into();
			}

			let fee = transaction.gas_price.saturating_mul(transaction.gas_limit);

			if account_data.balance < fee {
				return InvalidTransaction::Payment.into();
			}

			let mut builder = ValidTransactionBuilder::default()
				.and_provides((origin, transaction.nonce));

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

impl<T: Trait> Module<T> {
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

	fn store_block() {
		let mut transactions = Vec::new();
		let mut statuses = Vec::new();
		let mut receipts = Vec::new();
		for (transaction, status, receipt) in Pending::get() {
			transactions.push(transaction);
			statuses.push(status);
			receipts.push(receipt);
		}

		let ommers = Vec::<ethereum::Header>::new();
		let partial_header = ethereum::PartialHeader {
			parent_hash: Self::current_block_hash().unwrap_or_default(),
			beneficiary: <Module<T>>::find_author(),
			// TODO: figure out if there's better way to get a sort-of-valid state root.
			state_root: H256::default(),
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
			gas_used: receipts.clone().into_iter().fold(U256::zero(), |acc, r| acc + r.used_gas),
			timestamp: UniqueSaturatedInto::<u64>::unique_saturated_into(
				pallet_timestamp::Module::<T>::get()
			),
			extra_data: Vec::new(),
			mix_hash: H256::default(),
			nonce: H64::default(),
		};
		let block = ethereum::Block::new(partial_header, transactions.clone(), ommers);

		let mut transaction_hashes = Vec::new();

		for t in &transactions {
			let transaction_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(t)).as_slice()
			);
			transaction_hashes.push(transaction_hash);
		}

		CurrentBlock::put(block.clone());
		CurrentReceipts::put(receipts.clone());
		CurrentTransactionStatuses::put(statuses.clone());

		let digest = DigestItem::<T::Hash>::Consensus(
			FRONTIER_ENGINE_ID,
			ConsensusLog::EndBlock {
				block_hash: block.header.hash(),
				transaction_hashes,
			}.encode(),
		);
		frame_system::Module::<T>::deposit_log(digest.into());
	}

	/// Get the author using the FindAuthor trait.
	pub fn find_author() -> H160 {
		let digest = <frame_system::Module<T>>::digest();
		let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());

		T::FindAuthor::find_author(pre_runtime_digests).unwrap_or_default()
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

	/// Execute an Ethereum transaction, ignoring transaction signatures.
	pub fn execute(
		from: H160,
		input: Vec<u8>,
		value: U256,
		gas_limit: U256,
		gas_price: Option<U256>,
		nonce: Option<U256>,
		action: TransactionAction,
	) -> Result<(Option<H160>, CallOrCreateInfo), DispatchError> {
		match action {
			ethereum::TransactionAction::Call(target) => {
				Ok((Some(target), CallOrCreateInfo::Call(T::Runner::call(
					from,
					target,
					input.clone(),
					value,
					gas_limit.low_u32(),
					gas_price,
					nonce,
				).map_err(Into::into)?)))
			},
			ethereum::TransactionAction::Create => {
				Ok((None, CallOrCreateInfo::Create(T::Runner::create(
					from,
					input.clone(),
					value,
					gas_limit.low_u32(),
					gas_price,
					nonce,
				).map_err(Into::into)?)))
			},
		}
	}
}
