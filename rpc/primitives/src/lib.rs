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

use sp_core::{H160, H256, H512, U256};
use ethereum::{Log, Block as EthereumBlock};
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
pub struct Transaction {
	pub hash: H256,
	pub nonce: U256,
	pub block_hash: Option<H256>,
	pub block_number: Option<U256>,
	pub transaction_index: Option<U256>,
	pub from: H160,
	pub to: Option<H160>,
	pub value: U256,
	pub gas_price: U256,
	pub gas: U256,
	pub input: Vec<u8>,
	pub creates: Option<H160>,
	pub raw: Vec<u8>,
	pub public_key: Option<H512>,
	pub chain_id: Option<u64>,
	pub standard_v: U256,
	pub v: U256,
	pub r: U256,
	pub s: U256,
	pub condition: Option<u64>,
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
		fn block_by_number(number: u32) -> Option<EthereumBlock>;
		fn transaction_by_hash(hash: H256) -> Option<Transaction>;
	}
}

pub trait ConvertTransaction<E> {
	fn convert_transaction(&self, transaction: ethereum::Transaction) -> E;
}
