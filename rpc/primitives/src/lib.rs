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
	pub trait EthereumRuntimeRPCApi {
		/// Returns pallet_evm::Accounts by address.
		fn account_basic(address: H160) -> sp_evm::Account;
		/// Returns a frame_ethereum::call response.
		fn call(
			from: H160,
			to: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
		) -> Result<sp_evm::CallInfo, sp_runtime::DispatchError>;
		/// Returns a frame_ethereum::create response.
		fn create(
			from: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
		) -> Result<sp_evm::CreateInfo, sp_runtime::DispatchError>;
	}
}

pub trait ConvertTransaction<E> {
	fn convert_transaction(&self, transaction: ethereum::Transaction) -> E;
}
