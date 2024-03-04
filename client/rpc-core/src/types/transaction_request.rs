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

use ethereum::{
	AccessListItem, EIP1559TransactionMessage, EIP2930TransactionMessage, LegacyTransactionMessage,
	TransactionAction,
};
use ethereum_types::{H160, U256, U64};
use serde::{Deserialize, Deserializer};

use crate::types::Bytes;

/// Transaction request from the RPC.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRequest {
	/// Sender
	pub from: Option<H160>,
	/// Recipient
	pub to: Option<H160>,

	/// Value of transaction in wei
	pub value: Option<U256>,
	/// Transaction's nonce
	pub nonce: Option<U256>,
	/// Gas limit
	pub gas: Option<U256>,

	/// The gas price willing to be paid by the sender in wei
	pub gas_price: Option<U256>,
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee and miner / priority fee) in wei
	pub max_fee_per_gas: Option<U256>,
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	pub max_priority_fee_per_gas: Option<U256>,

	/// Additional data
	#[serde(flatten)]
	pub data: Data,

	/// EIP-2930 access list
	pub access_list: Option<Vec<AccessListItem>>,
	/// Chain ID that this transaction is valid on
	pub chain_id: Option<U64>,

	/// EIP-2718 type
	#[serde(rename = "type")]
	pub transaction_type: Option<U256>,
}

impl TransactionRequest {
	// We accept "data" and "input" for backwards-compatibility reasons.
	// "input" is the newer name and should be preferred by clients.
	/// Return the additional data of the transaction.
	pub fn data(&self) -> Option<&Bytes> {
		match (&self.data.input, &self.data.data) {
			(Some(input), _) => Some(input),
			(None, Some(data)) => Some(data),
			(None, None) => None,
		}
	}
}

/// Additional data of the transaction.
// We accept "data" and "input" for backwards-compatibility reasons.
// "input" is the newer name and should be preferred by clients.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Data {
	/// Additional data
	pub input: Option<Bytes>,
	/// Additional data
	pub data: Option<Bytes>,
}

impl Data {
	// We accept "data" and "input" for backwards-compatibility reasons.
	// "input" is the newer name and should be preferred by clients.
	/// Return the additional data of the transaction.
	pub fn into_bytes(self) -> Option<Bytes> {
		match (self.input, self.data) {
			(Some(input), _) => Some(input),
			(None, Some(data)) => Some(data),
			(None, None) => None,
		}
	}
}

impl<'de> Deserialize<'de> for Data {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		#[derive(Deserialize)]
		struct InputOrData {
			input: Option<Bytes>,
			data: Option<Bytes>,
		}

		let InputOrData { input, data } = InputOrData::deserialize(deserializer)?;

		match (input, data) {
			(Some(input), Some(data)) => {
				if input == data {
					Ok(Self {
						input: Some(input),
						data: Some(data),
					})
				} else {
					Err(serde::de::Error::custom(
						"Ambiguous value for `data` and `input`".to_string(),
					))
				}
			}
			(input, data) => Ok(Self { input, data }),
		}
	}
}

pub enum TransactionMessage {
	Legacy(LegacyTransactionMessage),
	EIP2930(EIP2930TransactionMessage),
	EIP1559(EIP1559TransactionMessage),
}

impl From<TransactionRequest> for Option<TransactionMessage> {
	fn from(req: TransactionRequest) -> Self {
		match (req.max_fee_per_gas, &req.access_list, req.gas_price) {
			// EIP1559
			// Empty fields fall back to the canonical transaction schema.
			(Some(_), _, None) | (None, None, None) => {
				Some(TransactionMessage::EIP1559(EIP1559TransactionMessage {
					action: match req.to {
						Some(to) => TransactionAction::Call(to),
						None => TransactionAction::Create,
					},
					nonce: req.nonce.unwrap_or_default(),
					max_priority_fee_per_gas: req.max_priority_fee_per_gas.unwrap_or_default(),
					max_fee_per_gas: req.max_fee_per_gas.unwrap_or_default(),
					gas_limit: req.gas.unwrap_or_default(),
					value: req.value.unwrap_or_default(),
					input: req
						.data
						.into_bytes()
						.map(|bytes| bytes.into_vec())
						.unwrap_or_default(),
					access_list: req.access_list.unwrap_or_default(),
					chain_id: req.chain_id.map(|id| id.as_u64()).unwrap_or_default(),
				}))
			}
			// EIP2930
			(None, Some(_), _) => Some(TransactionMessage::EIP2930(EIP2930TransactionMessage {
				action: match req.to {
					Some(to) => TransactionAction::Call(to),
					None => TransactionAction::Create,
				},
				nonce: req.nonce.unwrap_or_default(),
				gas_price: req.gas_price.unwrap_or_default(),
				gas_limit: req.gas.unwrap_or_default(),
				value: req.value.unwrap_or_default(),
				input: req
					.data
					.into_bytes()
					.map(|bytes| bytes.into_vec())
					.unwrap_or_default(),
				access_list: req.access_list.unwrap_or_default(),
				chain_id: req.chain_id.map(|id| id.as_u64()).unwrap_or_default(),
			})),
			// Legacy
			(None, None, Some(gas_price)) => {
				Some(TransactionMessage::Legacy(LegacyTransactionMessage {
					action: match req.to {
						Some(to) => TransactionAction::Call(to),
						None => TransactionAction::Create,
					},
					nonce: req.nonce.unwrap_or_default(),
					gas_price,
					gas_limit: req.gas.unwrap_or_default(),
					value: req.value.unwrap_or_default(),
					input: req
						.data
						.into_bytes()
						.map(|bytes| bytes.into_vec())
						.unwrap_or_default(),
					chain_id: None,
				}))
			}
			// Invalid parameter
			_ => None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn test_deserialize_with_only_input() {
		let data = json!({
			"from": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b",
			"to": "0x13fe2d1d3665660d22ff9624b7be0551ee1ac91b",
			"gasPrice": "0x10",
			"maxFeePerGas": "0x20",
			"maxPriorityFeePerGas": "0x30",
			"gas": "0x40",
			"value": "0x50",
			"input": "0x123abc",
			"nonce": "0x60",
			"accessList": [{"address": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b", "storageKeys": []}],
			"type": "0x70"
		});

		let args = serde_json::from_value::<TransactionRequest>(data).unwrap();
		assert_eq!(
			args.data,
			Data {
				input: Some(Bytes::from(vec![0x12, 0x3a, 0xbc])),
				data: None,
			}
		);
	}

	#[test]
	fn test_deserialize_with_only_data() {
		let data = json!({
			"from": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b",
			"to": "0x13fe2d1d3665660d22ff9624b7be0551ee1ac91b",
			"gasPrice": "0x10",
			"maxFeePerGas": "0x20",
			"maxPriorityFeePerGas": "0x30",
			"gas": "0x40",
			"value": "0x50",
			"data": "0x123abc",
			"nonce": "0x60",
			"accessList": [{"address": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b", "storageKeys": []}],
			"type": "0x70"
		});

		let args = serde_json::from_value::<TransactionRequest>(data).unwrap();
		assert_eq!(
			args.data,
			Data {
				input: None,
				data: Some(Bytes::from(vec![0x12, 0x3a, 0xbc])),
			}
		);
	}

	#[test]
	fn test_deserialize_with_data_and_input_mismatch() {
		let data = json!({
			"from": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b",
			"to": "0x13fe2d1d3665660d22ff9624b7be0551ee1ac91b",
			"gasPrice": "0x10",
			"maxFeePerGas": "0x20",
			"maxPriorityFeePerGas": "0x30",
			"gas": "0x40",
			"value": "0x50",
			"data": "0x123abc",
			"input": "0x456def",
			"nonce": "0x60",
			"accessList": [{"address": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b", "storageKeys": []}],
			"type": "0x70"
		});

		let args = serde_json::from_value::<TransactionRequest>(data);
		assert!(args.is_err());
	}

	#[test]
	fn test_deserialize_with_data_and_input_equal() {
		let data = json!({
			"from": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b",
			"to": "0x13fe2d1d3665660d22ff9624b7be0551ee1ac91b",
			"gasPrice": "0x10",
			"maxFeePerGas": "0x20",
			"maxPriorityFeePerGas": "0x30",
			"gas": "0x40",
			"value": "0x50",
			"data": "0x123abc",
			"input": "0x123abc",
			"nonce": "0x60",
			"accessList": [{"address": "0x60be2d1d3665660d22ff9624b7be0551ee1ac91b", "storageKeys": []}],
			"type": "0x70"
		});

		let args = serde_json::from_value::<TransactionRequest>(data).unwrap();
		assert_eq!(
			args.data,
			Data {
				input: Some(Bytes::from(vec![0x12, 0x3a, 0xbc])),
				data: Some(Bytes::from(vec![0x12, 0x3a, 0xbc])),
			}
		);
	}
}
