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

#![cfg_attr(not(feature = "std"), no_std)]

use sp_core::{H160, H256, U256};
use ethereum::{
	Log, Block as EthereumBlock, Transaction as EthereumTransaction,
	Receipt as EthereumReceipt, TransactionAction
};
use ethereum_types::Bloom;
use codec::{Encode, Decode};
use sp_std::vec::Vec;

#[derive(Eq, PartialEq, Clone, Encode, Decode, sp_runtime::RuntimeDebug)]
pub struct TransactionStatus {
	pub transaction_hash: H256,
	pub transaction_index: u32,
	pub from: H160,
	pub to: Option<H160>,
	pub contract_address: Option<H160>,
	pub logs: Vec<Log>,
	pub logs_bloom: Bloom,
}

impl Default for TransactionStatus {
	fn default() -> Self {
		TransactionStatus {
			transaction_hash: H256::default(),
			transaction_index: 0 as u32,
			from: H160::default(),
			to: None,
			contract_address: None,
			logs: Vec::new(),
			logs_bloom: Bloom::default(),
		}
	}
}

sp_api::decl_runtime_apis! {
	/// API necessary for Ethereum-compatibility layer.
	pub trait EthereumRuntimeApi {
		/// Returns runtime defined pallet_evm::ChainId.
		fn chain_id() -> u64;
		/// Returns pallet_evm::Accounts by address.
		fn account_basic(address: H160) -> pallet_evm::Account;
		/// Returns FixedGasPrice::min_gas_price
		fn gas_price() -> U256;
		/// For a given account address, returns pallet_evm::AccountCodes.
		fn account_code_at(address: H160) -> Vec<u8>;
		/// Returns the converted FindAuthor::find_author authority id.
		fn author() -> H160;
		/// For a given account address and index, returns pallet_evm::AccountStorages.
		fn storage_at(address: H160, index: U256) -> H256;
		/// Returns a pallet_evm::execute_call response.
		fn call(
			from: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: U256,
			nonce: Option<U256>,
			action: TransactionAction
		) -> Option<(Vec<u8>, U256)>;
		/// For a given block number, returns an ethereum::Block and all its TransactionStatus.
		fn block_by_number(number: u32) -> (Option<EthereumBlock>, Vec<Option<TransactionStatus>>);
		/// For a given block number, returns the number of transactions.
		fn block_transaction_count_by_number(number: u32) -> Option<U256>;
		/// For a given block hash, returns an ethereum::Block.
		fn block_by_hash(hash: H256) -> Option<EthereumBlock>;
		/// For a given block hash, returns an ethereum::Block and all its TransactionStatus.
		fn block_by_hash_with_statuses(hash: H256) -> (Option<EthereumBlock>, Vec<Option<TransactionStatus>>);
		/// For a given block hash, returns the number of transactions in a given block hash.
		fn block_transaction_count_by_hash(hash: H256) -> Option<U256>;
		/// For a given transaction hash, returns data necessary to build an Transaction rpc type response.
		/// - EthereumTransaction: transaction as stored in pallet-ethereum.
		/// - EthereumBlock: block as stored in pallet-ethereum .
		/// - TransactionStatus: transaction execution metadata.
		/// - EthereumReceipt: transaction receipt.
		fn transaction_by_hash(hash: H256) -> Option<(
			EthereumTransaction,
			EthereumBlock,
			TransactionStatus,
			Vec<EthereumReceipt>
		)>;
		/// For a given block hash and transaction index, returns data necessary to build an Transaction rpc
		/// type response.
		/// - EthereumTransaction: transaction as stored in pallet-ethereum.
		/// - EthereumBlock: block as stored in pallet-ethereum .
		/// - TransactionStatus: transaction execution metadata.
		fn transaction_by_block_hash_and_index(
			hash: H256,
			index: u32
		) -> Option<(
			EthereumTransaction,
			EthereumBlock,
			TransactionStatus
		)>;
		/// For a given block number and transaction index, returns data necessary to build an Transaction rpc
		/// type response.
		/// - EthereumTransaction: transaction as stored in pallet-ethereum.
		/// - EthereumBlock: block as stored in pallet-ethereum .
		/// - TransactionStatus: transaction execution metadata.
		fn transaction_by_block_number_and_index(
			number: u32,
			index: u32
		) -> Option<(
			EthereumTransaction,
			EthereumBlock,
			TransactionStatus
		)>;
		/// For given filter arguments, return data necessary to build Logs
		fn logs(
			from_block: Option<u32>,
			to_block: Option<u32>,
			block_hash: Option<H256>,
			address: Option<H160>,
			topic: Option<Vec<H256>>
		) -> Vec<(
			H160, // address
			Vec<H256>, // topics
			Vec<u8>, // data
			Option<H256>, // block_hash
			Option<U256>, // block_number
			Option<H256>, // transaction_hash
			Option<U256>, // transaction_index
			Option<U256>, // log index in block
			Option<U256>, // log index in transaction
		)>;
	}
}

pub trait ConvertTransaction<E> {
	fn convert_transaction(&self, transaction: ethereum::Transaction) -> E;
}
