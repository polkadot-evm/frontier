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
use ethereum::Log;
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

#[derive(Eq, PartialEq, Clone, Encode, Decode, sp_runtime::RuntimeDebug)]
pub struct PrimitiveBlock {
	pub hash: Option<H256>, // TODO not in ethereum::Block
	pub parent_hash: H256,
	pub uncles_hash: H256, // TODO not in ethereum::Block
	pub author: H160, // TODO not in ethereum::Block
	pub miner: H160, // TODO not in ethereum::Block
	pub state_root: H256,
	pub transactions_root: H256,
	pub receipts_root: H256,
	pub number: Option<U256>,
	pub gas_used: U256,
	pub gas_limit: U256,
	pub extra_data: Vec<u8>,
	pub logs_bloom: Option<Bloom>,
	pub timestamp: U256,
	pub difficulty: U256,
	pub total_difficulty: Option<U256>, // TODO not in ethereum::Block
	pub seal_fields: Vec<Vec<u8>>, // TODO not in ethereum::Block
	pub uncles: Vec<H256>, // TODO not in ethereum::Block
	pub transactions: Vec<H256>, // ? TODO need support for full txns and txn hashes
	pub size: Option<U256>, // TODO not in ethereum::Block
}

sp_api::decl_runtime_apis! {
	/// API necessary for Ethereum-compatibility layer.
	pub trait EthereumRuntimeApi {
		fn chain_id() -> u64;
		fn account_basic(address: H160) -> pallet_evm::Account;
		fn transaction_status(hash: H256) -> Option<TransactionStatus>;
		fn gas_price() -> U256;
		fn account_code_at(address: H160) -> Vec<u8>;
		fn author() -> H160;
		fn block_by_number(number: u32) -> Option<PrimitiveBlock>;
	}
}

pub trait ConvertTransaction<E> {
	fn convert_transaction(&self, transaction: ethereum::Transaction) -> E;
}
