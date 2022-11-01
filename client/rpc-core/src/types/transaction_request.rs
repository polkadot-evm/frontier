// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2022 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! `TransactionRequest` type

use crate::types::Bytes;
use ethereum::{
	AccessListItem, EIP1559TransactionMessage, EIP2930TransactionMessage, LegacyTransactionMessage,
};
use ethereum_types::{H160, U256};
use serde::{Deserialize, Serialize};

pub enum TransactionMessage {
	Legacy(LegacyTransactionMessage),
	EIP2930(EIP2930TransactionMessage),
	EIP1559(EIP1559TransactionMessage),
}

/// Transaction request coming from RPC
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRequest {
	/// Sender
	pub from: Option<H160>,
	/// Recipient
	pub to: Option<H160>,
	/// Gas Price, legacy.
	#[serde(default)]
	pub gas_price: Option<U256>,
	/// Max BaseFeePerGas the user is willing to pay.
	#[serde(default)]
	pub max_fee_per_gas: Option<U256>,
	/// The miner's tip.
	#[serde(default)]
	pub max_priority_fee_per_gas: Option<U256>,
	/// Gas
	pub gas: Option<U256>,
	/// Value of transaction in wei
	pub value: Option<U256>,
	/// Additional data sent with transaction
	pub data: Option<Bytes>,
	/// Transaction's nonce
	pub nonce: Option<U256>,
	/// Pre-pay to warm storage access.
	#[serde(default)]
	pub access_list: Option<Vec<AccessListItem>>,
	/// EIP-2718 type
	#[serde(rename = "type")]
	pub transaction_type: Option<U256>,
}

impl From<TransactionRequest> for Option<TransactionMessage> {
	fn from(req: TransactionRequest) -> Self {
		match (req.gas_price, req.max_fee_per_gas, req.access_list.clone()) {
			// Legacy
			(Some(_), None, None) => Some(TransactionMessage::Legacy(LegacyTransactionMessage {
				nonce: U256::zero(),
				gas_price: req.gas_price.unwrap_or_default(),
				gas_limit: req.gas.unwrap_or_default(),
				value: req.value.unwrap_or_default(),
				input: req.data.map(|s| s.into_vec()).unwrap_or_default(),
				action: match req.to {
					Some(to) => ethereum::TransactionAction::Call(to),
					None => ethereum::TransactionAction::Create,
				},
				chain_id: None,
			})),
			// EIP2930
			(_, None, Some(_)) => Some(TransactionMessage::EIP2930(EIP2930TransactionMessage {
				nonce: U256::zero(),
				gas_price: req.gas_price.unwrap_or_default(),
				gas_limit: req.gas.unwrap_or_default(),
				value: req.value.unwrap_or_default(),
				input: req.data.map(|s| s.into_vec()).unwrap_or_default(),
				action: match req.to {
					Some(to) => ethereum::TransactionAction::Call(to),
					None => ethereum::TransactionAction::Create,
				},
				chain_id: 0,
				access_list: req.access_list.unwrap_or_default(),
			})),
			// EIP1559
			(None, Some(_), _) | (None, None, None) => {
				// Empty fields fall back to the canonical transaction schema.
				Some(TransactionMessage::EIP1559(EIP1559TransactionMessage {
					nonce: U256::zero(),
					max_fee_per_gas: req.max_fee_per_gas.unwrap_or_default(),
					max_priority_fee_per_gas: req.max_priority_fee_per_gas.unwrap_or_default(),
					gas_limit: req.gas.unwrap_or_default(),
					value: req.value.unwrap_or_default(),
					input: req.data.map(|s| s.into_vec()).unwrap_or_default(),
					action: match req.to {
						Some(to) => ethereum::TransactionAction::Call(to),
						None => ethereum::TransactionAction::Create,
					},
					chain_id: 0,
					access_list: req.access_list.unwrap_or_default(),
				}))
			}
			_ => None,
		}
	}
}
