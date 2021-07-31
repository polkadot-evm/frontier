// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2020 Parity Technologies (UK) Ltd.
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

use serde::{Serialize, Deserialize};
use ethereum_types::{H160, H256, U256};
use ethereum::{AccessListItem, TransactionMessage};
use crate::types::Bytes;

/// Transaction request coming from RPC
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
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
	/// TODO! Pre-pay to warm storage access.
	#[serde(default)]
	pub access_list: Option<Vec<(H160, Vec<H256>)>>,
}

impl Into<Option<TransactionMessage>> for TransactionRequest {
	fn into(self) -> Option<TransactionMessage> {
		match (self.gas_price, self.max_fee_per_gas, self.access_list.clone()) {
			(Some(_), None, None) => {
				// Legacy V0 transaction.
				Some(ethereum::TransactionMessage::V0(ethereum::TransactionMessageV0 {
					nonce: U256::zero(),
					// TODO the default was wrong, needed min_gas_price (old),
					// and new will fall back to BaseFeePerGas
					gas_price: self.gas_price.unwrap_or(U256::from(1)),
					gas_limit: self.gas.unwrap_or(U256::max_value()),
					value: self.value.unwrap_or(U256::zero()),
					input: self.data.map(|s| s.into_vec()).unwrap_or_default(),
					action: match self.to {
						Some(to) => ethereum::TransactionAction::Call(to),
						None => ethereum::TransactionAction::Create,
					},
					chain_id: None,
				}))
			},
			(_, None, Some(_)) => {
				// V1 transaction (EIP-2930).
				Some(ethereum::TransactionMessage::V1(ethereum::TransactionMessageV1 {
					nonce: U256::zero(),
					// TODO the default was wrong, needed min_gas_price (old),
					// and new will fall back to BaseFeePerGas
					gas_price: self.gas_price.unwrap_or(U256::from(1)),
					gas_limit: self.gas.unwrap_or(U256::max_value()),
					value: self.value.unwrap_or(U256::zero()),
					input: self.data.map(|s| s.into_vec()).unwrap_or_default(),
					action: match self.to {
						Some(to) => ethereum::TransactionAction::Call(to),
						None => ethereum::TransactionAction::Create,
					},
					chain_id: 0,
					access_list: self.access_list.unwrap().into_iter()
						.map(|(address, slots)| AccessListItem { address, slots })
						.collect(),
				}))
			},
			(None, Some(_), _) | (None, None, None) => {
				// V2 transaction (EIP-1559).
				// Empty fields fall back to the canonical transaction schema.
				Some(ethereum::TransactionMessage::V2(ethereum::TransactionMessageV2 {
					nonce: U256::zero(),
					// TODO the default must be BaseFeePerGas
					max_fee_per_gas: self.max_fee_per_gas.unwrap_or(U256::from(1)),
					max_priority_fee_per_gas: self.max_priority_fee_per_gas.unwrap_or(U256::from(0)),
					gas_limit: self.gas.unwrap_or(U256::max_value()),
					value: self.value.unwrap_or(U256::zero()),
					input: self.data.map(|s| s.into_vec()).unwrap_or_default(),
					action: match self.to {
						Some(to) => ethereum::TransactionAction::Call(to),
						None => ethereum::TransactionAction::Create,
					},
					chain_id: 0,
					access_list: self.access_list.unwrap_or(Vec::new()).into_iter()
						.map(|(address, slots)| AccessListItem { address, slots })
						.collect(),
				}))
			},
			_ => None
		}
	}
}
