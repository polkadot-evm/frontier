// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use ethereum::{
	AccessListItem, AuthorizationListItem, EIP1559TransactionMessage, EIP2930TransactionMessage,
	EIP7702TransactionMessage, LegacyTransactionMessage, TransactionAction,
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
	#[serde(with = "access_list_item_camelcase", default)]
	pub access_list: Option<Vec<AccessListItem>>,
	/// EIP-7702 authorization list
	#[serde(with = "authorization_list_item_camelcase", default)]
	pub authorization_list: Option<Vec<AuthorizationListItem>>,
	/// Chain ID that this transaction is valid on
	pub chain_id: Option<U64>,

	/// EIP-2718 type
	#[serde(rename = "type")]
	pub transaction_type: Option<U256>,
}

/// Fix broken unit-test due to the `serde(rename_all = "camelCase")` attribute of type [ethereum::AccessListItem] has been deleted.
/// Refer to this [commit](https://github.com/rust-ethereum/ethereum/commit/b160820620aa9fd30050d5fcb306be4e12d58c8c#diff-2a6a2a5c32456901be5ffa0e2d0354f2d48d96a89e486270ae62808c34b6e96f)
mod access_list_item_camelcase {
	use ethereum::AccessListItem;
	use ethereum_types::{Address, H256};
	use serde::{Deserialize, Deserializer};

	#[derive(Deserialize)]
	struct AccessListItemDef {
		address: Address,
		#[serde(rename = "storageKeys")]
		storage_keys: Vec<H256>,
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<AccessListItem>>, D::Error>
	where
		D: Deserializer<'de>,
	{
		let access_item_defs_opt: Option<Vec<AccessListItemDef>> =
			Option::deserialize(deserializer)?;
		Ok(access_item_defs_opt.map(|access_item_defs| {
			access_item_defs
				.into_iter()
				.map(|access_item_def| AccessListItem {
					address: access_item_def.address,
					storage_keys: access_item_def.storage_keys,
				})
				.collect()
		}))
	}
}

/// Serde support for AuthorizationListItem with camelCase field names
mod authorization_list_item_camelcase {
	use ethereum::{eip2930::MalleableTransactionSignature, AuthorizationListItem};
	use ethereum_types::{Address, H256};
	use serde::{Deserialize, Deserializer};

	#[derive(Deserialize)]
	struct AuthorizationListItemDef {
		#[serde(rename = "chainId")]
		chain_id: u64,
		address: Address,
		nonce: ethereum_types::U256,
		#[serde(rename = "yParity")]
		y_parity: bool,
		r: H256,
		s: H256,
	}

	pub fn deserialize<'de, D>(
		deserializer: D,
	) -> Result<Option<Vec<AuthorizationListItem>>, D::Error>
	where
		D: Deserializer<'de>,
	{
		let auth_item_defs_opt: Option<Vec<AuthorizationListItemDef>> =
			Option::deserialize(deserializer)?;
		Ok(auth_item_defs_opt.map(|auth_item_defs| {
			auth_item_defs
				.into_iter()
				.map(|auth_item_def| AuthorizationListItem {
					chain_id: auth_item_def.chain_id,
					address: auth_item_def.address,
					nonce: auth_item_def.nonce,
					signature: MalleableTransactionSignature {
						odd_y_parity: auth_item_def.y_parity,
						r: auth_item_def.r,
						s: auth_item_def.s,
					},
				})
				.collect()
		}))
	}
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

	/// Convert the transaction request's `to` field into a TransactionAction
	fn to_action(&self) -> TransactionAction {
		match self.to {
			Some(to) => TransactionAction::Call(to),
			None => TransactionAction::Create,
		}
	}

	/// Convert the transaction request's data field into bytes
	fn data_to_bytes(&self) -> Vec<u8> {
		self.data
			.clone()
			.into_bytes()
			.map(|bytes| bytes.into_vec())
			.unwrap_or_default()
	}

	/// Extract chain_id as u64
	fn chain_id_u64(&self) -> u64 {
		self.chain_id.map(|id| id.as_u64()).unwrap_or_default()
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
	EIP7702(EIP7702TransactionMessage),
}

impl From<TransactionRequest> for Option<TransactionMessage> {
	fn from(req: TransactionRequest) -> Self {
		// Common fields extraction - these are used by all transaction types
		let nonce = req.nonce.unwrap_or_default();
		let gas_limit = req.gas.unwrap_or_default();
		let value = req.value.unwrap_or_default();
		let action = req.to_action();
		let chain_id = req.chain_id_u64();
		let data_bytes = req.data_to_bytes();

		// Determine transaction type based on presence of fields
		let has_authorization_list = req.authorization_list.is_some();
		let has_access_list = req.access_list.is_some();
		let access_list = req.access_list.unwrap_or_default();

		match (
			req.max_fee_per_gas,
			has_access_list,
			req.gas_price,
			has_authorization_list,
		) {
			// EIP7702: Has authorization_list (takes priority)
			(_, _, _, true) => Some(TransactionMessage::EIP7702(EIP7702TransactionMessage {
				destination: action,
				nonce,
				max_priority_fee_per_gas: req.max_priority_fee_per_gas.unwrap_or_default(),
				max_fee_per_gas: req.max_fee_per_gas.unwrap_or_default(),
				gas_limit,
				value,
				data: data_bytes,
				access_list,
				authorization_list: req.authorization_list.unwrap(),
				chain_id,
			})),
			// EIP1559: Has max_fee_per_gas but no gas_price, or all fee fields are None
			(Some(_), _, None, false) | (None, false, None, false) => {
				Some(TransactionMessage::EIP1559(EIP1559TransactionMessage {
					action,
					nonce,
					max_priority_fee_per_gas: req.max_priority_fee_per_gas.unwrap_or_default(),
					max_fee_per_gas: req.max_fee_per_gas.unwrap_or_default(),
					gas_limit,
					value,
					input: data_bytes,
					access_list,
					chain_id,
				}))
			}
			// EIP2930: Has access_list but no max_fee_per_gas
			(None, true, _, false) => {
				Some(TransactionMessage::EIP2930(EIP2930TransactionMessage {
					action,
					nonce,
					gas_price: req.gas_price.unwrap_or_default(),
					gas_limit,
					value,
					input: data_bytes,
					access_list,
					chain_id,
				}))
			}
			// Legacy: Has gas_price but no access_list or max_fee_per_gas
			(None, false, Some(gas_price), false) => {
				Some(TransactionMessage::Legacy(LegacyTransactionMessage {
					action,
					nonce,
					gas_price,
					gas_limit,
					value,
					input: data_bytes,
					chain_id: None, // Legacy transactions don't include chain_id
				}))
			}
			// Invalid parameter combination
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
	fn test_deserialize_missing_field_access_list() {
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
