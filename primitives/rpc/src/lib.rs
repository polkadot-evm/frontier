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

#![cfg_attr(not(feature = "std"), no_std)]

use sp_core::{H160, H256, U256};
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
		/// Returns runtime defined pallet_evm::ChainId.
		fn chain_id() -> u64;
		/// Returns pallet_evm::Accounts by address.
		fn account_basic(address: H160) -> fp_evm::Account;
		/// Returns FixedGasPrice::min_gas_price
		fn gas_price() -> U256;
		/// For a given account address, returns pallet_evm::AccountCodes.
		fn account_code_at(address: H160) -> Vec<u8>;
		/// Returns the converted FindAuthor::find_author authority id.
		fn author() -> H160;
		/// For a given account address and index, returns pallet_evm::AccountStorages.
		fn storage_at(address: H160, index: U256) -> H256;
		/// Returns a frame_ethereum::call response.
		fn call(
			from: H160,
			to: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
		) -> Result<fp_evm::CallInfo, sp_runtime::DispatchError>;
		/// Returns a frame_ethereum::create response.
		fn create(
			from: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
		) -> Result<fp_evm::CreateInfo, sp_runtime::DispatchError>;
		/// Return the current block.
		fn current_block() -> Option<EthereumBlock>;
		/// Return the current receipt.
		fn current_receipts() -> Option<Vec<ethereum::Receipt>>;
		/// Return the current transaction status.
		fn current_transaction_statuses() -> Option<Vec<TransactionStatus>>;
		/// Return all the current data for a block in a single runtime call.
		fn current_all() -> (
			Option<EthereumBlock>,
			Option<Vec<ethereum::Receipt>>,
			Option<Vec<TransactionStatus>>
		);
	}
}

pub trait ConvertTransaction<E> {
	fn convert_transaction(&self, transaction: ethereum::Transaction) -> E;
}
