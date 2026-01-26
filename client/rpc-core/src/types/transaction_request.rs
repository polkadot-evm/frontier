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

/// The default byte size of a transaction slot (32 KiB).
///
/// Reference:
/// - geth: <https://github.com/ethereum/go-ethereum/blob/master/core/txpool/legacypool/legacypool.go> (`txSlotSize`)
/// - reth: <https://github.com/paradigmxyz/reth/blob/main/crates/transaction-pool/src/validate/constants.rs#L4>
pub const TX_SLOT_BYTE_SIZE: usize = 32 * 1024;

/// The default maximum size a single transaction can have (128 KiB).
/// This is the RLP-encoded size of the signed transaction.
///
/// Reference:
/// - geth: <https://github.com/ethereum/go-ethereum/blob/master/core/txpool/legacypool/legacypool.go> (`txMaxSize`)
/// - reth: <https://github.com/paradigmxyz/reth/blob/main/crates/transaction-pool/src/validate/constants.rs#L11>
pub const DEFAULT_MAX_TX_INPUT_BYTES: usize = 4 * TX_SLOT_BYTE_SIZE;

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
	#[serde(rename_all = "camelCase")]
	struct AuthorizationListItemDef {
		chain_id: u64,
		address: Address,
		nonce: ethereum_types::U256,
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

	/// Calculates the RLP-encoded size of the signed transaction for DoS protection.
	///
	/// This mirrors geth's `tx.Size()` and reth's `transaction.encoded_length()` which use
	/// actual RLP encoding to determine transaction size. We convert the request to its
	/// transaction message type and use the `encoded_len()` method from the ethereum crate.
	///
	/// Reference:
	/// - geth: <https://github.com/ethereum/go-ethereum/blob/master/core/types/transaction.go> (`tx.Size()`)
	/// - reth: <https://github.com/paradigmxyz/reth/blob/main/crates/transaction-pool/src/traits.rs> (`PoolTransaction::encoded_length()`)
	/// - alloy: <https://github.com/alloy-rs/alloy/blob/main/crates/consensus/src/transaction/eip1559.rs> (`rlp_encoded_fields_length()`)
	pub fn encoded_length(&self) -> usize {
		// Convert to transaction message and use the ethereum crate's encoded_len()
		let message: Option<TransactionMessage> = self.clone().into();

		match message {
			Some(TransactionMessage::Legacy(msg)) => {
				// Legacy: RLP([nonce, gasPrice, gasLimit, to, value, data, v, r, s])
				// v is variable (27/28 or chainId*2+35/36), r and s are 32 bytes each
				msg.encoded_len() + Self::SIGNATURE_RLP_OVERHEAD
			}
			Some(TransactionMessage::EIP2930(msg)) => {
				// EIP-2930: 0x01 || RLP([chainId, nonce, gasPrice, gasLimit, to, value, data, accessList, yParity, r, s])
				1 + msg.encoded_len() + Self::SIGNATURE_RLP_OVERHEAD
			}
			Some(TransactionMessage::EIP1559(msg)) => {
				// EIP-1559: 0x02 || RLP([chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList, yParity, r, s])
				1 + msg.encoded_len() + Self::SIGNATURE_RLP_OVERHEAD
			}
			Some(TransactionMessage::EIP7702(msg)) => {
				// EIP-7702: 0x04 || RLP([chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList, authorizationList, yParity, r, s])
				1 + msg.encoded_len() + Self::SIGNATURE_RLP_OVERHEAD
			}
			None => {
				// Fallback for invalid/incomplete requests - use conservative estimate
				// This shouldn't happen in normal operation as validation should catch it
				Self::DEFAULT_FALLBACK_SIZE
			}
		}
	}

	/// RLP overhead for signature fields (yParity + r + s)
	/// - yParity: 1 byte (0x00 or 0x01 encoded as single byte)
	/// - r: typically 33 bytes (0x80 + 32 bytes, or less if leading zeros)
	/// - s: typically 33 bytes (0x80 + 32 bytes, or less if leading zeros)
	const SIGNATURE_RLP_OVERHEAD: usize = 1 + 33 + 33;

	/// Fallback size for invalid requests that can't be converted to a message
	const DEFAULT_FALLBACK_SIZE: usize = 256;

	/// Validates that the estimated signed transaction size is within limits.
	///
	/// This prevents DoS attacks via oversized transactions before they enter the pool.
	/// The limit matches geth's `txMaxSize` and reth's `DEFAULT_MAX_TX_INPUT_BYTES`.
	///
	/// Reference:
	/// - geth: <https://github.com/ethereum/go-ethereum/blob/master/core/txpool/validation.go> (`ValidateTransaction`)
	/// - reth: <https://github.com/paradigmxyz/reth/blob/main/crates/transaction-pool/src/validate/eth.rs#L342-L363>
	pub fn validate_size(&self) -> Result<(), String> {
		let size = self.encoded_length();

		if size > DEFAULT_MAX_TX_INPUT_BYTES {
			return Err(format!(
				"oversized data: transaction size {} exceeds limit {}",
				size, DEFAULT_MAX_TX_INPUT_BYTES
			));
		}
		Ok(())
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
				authorization_list: req.authorization_list.unwrap_or_default(),
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

	#[test]
	fn test_request_size_validation_large_access_list() {
		use ethereum::AccessListItem;
		use ethereum_types::{H160, H256};

		// Create access list that exceeds 128KB (131,072 bytes)
		// Each storage key RLP-encodes to ~33 bytes
		// 4000 keys * 33 bytes = 132,000 bytes > 128KB
		let storage_keys: Vec<H256> = (0..4000).map(|_| H256::default()).collect();
		let access_list = vec![AccessListItem {
			address: H160::default(),
			storage_keys,
		}];
		let request = TransactionRequest {
			access_list: Some(access_list),
			..Default::default()
		};
		assert!(request.validate_size().is_err());
	}

	#[test]
	fn test_request_size_validation_valid() {
		use ethereum::AccessListItem;
		use ethereum_types::{H160, H256};

		// 100 storage keys is well under 128KB
		let request = TransactionRequest {
			access_list: Some(vec![AccessListItem {
				address: H160::default(),
				storage_keys: vec![H256::default(); 100],
			}]),
			..Default::default()
		};
		assert!(request.validate_size().is_ok());
	}

	#[test]
	fn test_encoded_length_includes_signature_overhead() {
		// A minimal EIP-1559 transaction should include signature overhead
		// Default TransactionRequest converts to EIP-1559 (no gas_price, no access_list)
		let request = TransactionRequest::default();
		let size = request.encoded_length();

		// EIP-1559 message RLP: ~11 bytes for minimal fields (all zeros/empty)
		// + 1 byte type prefix + 67 bytes signature overhead = ~79 bytes minimum
		// The signature overhead (67 bytes) is the key verification
		assert!(
			size >= TransactionRequest::SIGNATURE_RLP_OVERHEAD,
			"Size {} should be at least signature overhead {}",
			size,
			TransactionRequest::SIGNATURE_RLP_OVERHEAD
		);

		// Verify it's a reasonable size for a minimal transaction
		assert!(
			size < 200,
			"Size {} should be reasonable for minimal tx",
			size
		);
	}

	#[test]
	fn test_encoded_length_typed_transaction_overhead() {
		use ethereum::AccessListItem;
		use ethereum_types::H160;

		// EIP-1559 transaction (has max_fee_per_gas)
		let request = TransactionRequest {
			max_fee_per_gas: Some(U256::from(1000)),
			access_list: Some(vec![AccessListItem {
				address: H160::default(),
				storage_keys: vec![],
			}]),
			..Default::default()
		};
		let typed_size = request.encoded_length();

		// Legacy transaction
		let legacy_request = TransactionRequest {
			gas_price: Some(U256::from(1000)),
			..Default::default()
		};
		let legacy_size = legacy_request.encoded_length();

		// Typed transaction should be larger due to:
		// - Type byte (+1)
		// - Chain ID (+9)
		// - max_priority_fee_per_gas (+33)
		// - Access list overhead
		assert!(
			typed_size > legacy_size,
			"Typed tx {} should be larger than legacy {}",
			typed_size,
			legacy_size
		);
	}

	#[test]
	fn test_encoded_length_access_list_scaling() {
		use ethereum::AccessListItem;
		use ethereum_types::{H160, H256};

		// Transaction with 10 storage keys
		let request_10 = TransactionRequest {
			access_list: Some(vec![AccessListItem {
				address: H160::default(),
				storage_keys: vec![H256::default(); 10],
			}]),
			..Default::default()
		};

		// Transaction with 100 storage keys
		let request_100 = TransactionRequest {
			access_list: Some(vec![AccessListItem {
				address: H160::default(),
				storage_keys: vec![H256::default(); 100],
			}]),
			..Default::default()
		};

		let size_10 = request_10.encoded_length();
		let size_100 = request_100.encoded_length();

		// Size should scale roughly linearly with storage keys
		// 90 additional keys * ~34 bytes each ≈ 3060 bytes difference
		let diff = size_100 - size_10;
		assert!(
			diff > 2500 && diff < 4000,
			"Size difference {} should be proportional to storage keys",
			diff
		);
	}

	#[test]
	fn test_constants_match_geth_reth() {
		// Verify our constants match geth/reth exactly
		assert_eq!(TX_SLOT_BYTE_SIZE, 32 * 1024); // 32 KiB
		assert_eq!(DEFAULT_MAX_TX_INPUT_BYTES, 128 * 1024); // 128 KiB
		assert_eq!(DEFAULT_MAX_TX_INPUT_BYTES, 4 * TX_SLOT_BYTE_SIZE);
	}
}
