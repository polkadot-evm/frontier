// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

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
#![warn(unused_crate_dependencies)]

extern crate alloc;

use alloc::vec::Vec;
pub use ethereum::{
	AccessListItem, AuthorizationList, AuthorizationListItem, BlockV3 as Block,
	LegacyTransactionMessage, Log, ReceiptV3 as Receipt, TransactionAction,
	TransactionV3 as Transaction,
};
use ethereum_types::{H160, H256, U256};
use fp_evm::{CallOrCreateInfo, CheckEvmTransactionInput};
use frame_support::dispatch::{DispatchErrorWithPostInfo, PostDispatchInfo};
use scale_codec::{Decode, Encode};

pub trait ValidatedTransaction {
	fn apply(
		source: H160,
		transaction: Transaction,
	) -> Result<(PostDispatchInfo, CallOrCreateInfo), DispatchErrorWithPostInfo>;
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
pub struct TransactionData {
	pub action: TransactionAction,
	pub input: Vec<u8>,
	pub nonce: U256,
	pub gas_limit: U256,
	pub gas_price: Option<U256>,
	pub max_fee_per_gas: Option<U256>,
	pub max_priority_fee_per_gas: Option<U256>,
	pub value: U256,
	pub chain_id: Option<u64>,
	pub access_list: Vec<(H160, Vec<H256>)>,
	pub authorization_list: AuthorizationList,
}

impl TransactionData {
	#[allow(clippy::too_many_arguments)]
	pub fn new(
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
		authorization_list: AuthorizationList,
	) -> Self {
		Self {
			action,
			input,
			nonce,
			gas_limit,
			gas_price,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			value,
			chain_id,
			access_list,
			authorization_list,
		}
	}

	// The transact call wrapped in the extrinsic is part of the PoV, record this as a base cost for the size of the proof.
	pub fn proof_size_base_cost(&self) -> u64 {
		self.encode()
			.len()
			// signature
			.saturating_add(65)
			// pallet index
			.saturating_add(1)
			// call index
			.saturating_add(1) as u64
	}
}

impl From<TransactionData> for CheckEvmTransactionInput {
	fn from(t: TransactionData) -> Self {
		CheckEvmTransactionInput {
			to: if let TransactionAction::Call(to) = t.action {
				Some(to)
			} else {
				None
			},
			chain_id: t.chain_id,
			input: t.input,
			nonce: t.nonce,
			gas_limit: t.gas_limit,
			gas_price: t.gas_price,
			max_fee_per_gas: t.max_fee_per_gas,
			max_priority_fee_per_gas: t.max_priority_fee_per_gas,
			value: t.value,
			access_list: t.access_list,
			authorization_list: t
				.authorization_list
				.iter()
				.map(|d| {
					(
						d.chain_id.into(),
						d.address,
						d.nonce,
						d.authorizing_address().ok(),
					)
				})
				.collect(),
		}
	}
}

impl From<&Transaction> for TransactionData {
	fn from(t: &Transaction) -> Self {
		match t {
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
				authorization_list: Vec::new(),
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
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
				authorization_list: Vec::new(),
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
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
				authorization_list: Vec::new(),
			},
			Transaction::EIP7702(t) => TransactionData {
				action: t.destination,
				input: t.data.clone(),
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
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
				authorization_list: t.authorization_list.clone(),
			},
		}
	}
}
