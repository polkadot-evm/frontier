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

use ethereum::{AccessListItem, TransactionAction, TransactionV3 as EthereumTransaction};
use ethereum_types::{Address, H160, H256, U256, U64};
use serde::{ser::SerializeStruct, Serialize, Serializer};

use crate::types::{BuildFrom, Bytes};

/// AuthorizationListItem for EIP-7702 transactions
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationListItem {
	pub chain_id: U64,
	pub address: Address,
	pub nonce: U256,
	pub y_parity: U64,
	pub r: U256,
	pub s: U256,
}

/// Transaction
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
	/// EIP-2718 transaction type
	#[serde(rename = "type")]
	pub transaction_type: U256,
	/// Hash
	pub hash: H256,
	/// Nonce
	pub nonce: U256,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Block number
	pub block_number: Option<U256>,
	/// Transaction Index
	pub transaction_index: Option<U256>,
	/// Sender
	pub from: H160,
	/// Recipient
	pub to: Option<H160>,
	/// Transferred value
	pub value: U256,
	/// Gas
	pub gas: U256,
	/// Gas Price
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U256>,
	/// Max BaseFeePerGas the user is willing to pay.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	/// The miner's tip.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_priority_fee_per_gas: Option<U256>,
	/// Data
	pub input: Bytes,
	/// Creates contract
	pub creates: Option<H160>,
	/// The network id of the transaction, if any.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U64>,
	/// Pre-pay to warm storage access.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub access_list: Option<Vec<AccessListItem>>,
	/// EIP-7702 authorization list.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub authorization_list: Option<Vec<AuthorizationListItem>>,
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub y_parity: Option<U256>,
	/// The standardised V field of the signature.
	///
	/// For backwards compatibility, `v` is optionally provided as an alternative to `yParity`.
	/// This field is DEPRECATED and all use of it should migrate to `yParity`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub v: Option<U256>,
	/// The R field of the signature.
	pub r: U256,
	/// The S field of the signature.
	pub s: U256,
}

impl BuildFrom for Transaction {
	fn build_from(from: H160, transaction: &EthereumTransaction) -> Self {
		let hash = transaction.hash();
		match transaction {
			EthereumTransaction::Legacy(t) => Self {
				transaction_type: U256::from(0),
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from,
				to: match t.action {
					TransactionAction::Call(to) => Some(to),
					TransactionAction::Create => None,
				},
				value: t.value,
				gas: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				input: Bytes(t.input.clone()),
				creates: None,
				chain_id: t.signature.chain_id().map(U64::from),
				access_list: None,
				authorization_list: None,
				y_parity: None,
				v: Some(U256::from(t.signature.v())),
				r: U256::from_big_endian(t.signature.r().as_bytes()),
				s: U256::from_big_endian(t.signature.s().as_bytes()),
			},
			EthereumTransaction::EIP2930(t) => Self {
				transaction_type: U256::from(1),
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from,
				to: match t.action {
					TransactionAction::Call(to) => Some(to),
					TransactionAction::Create => None,
				},
				value: t.value,
				gas: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				input: Bytes(t.input.clone()),
				creates: None,
				chain_id: Some(U64::from(t.chain_id)),
				access_list: Some(t.access_list.clone()),
				authorization_list: None,
				y_parity: Some(U256::from(t.signature.odd_y_parity() as u8)),
				v: Some(U256::from(t.signature.odd_y_parity() as u8)),
				r: U256::from_big_endian(t.signature.r().as_bytes()),
				s: U256::from_big_endian(t.signature.s().as_bytes()),
			},
			EthereumTransaction::EIP1559(t) => Self {
				transaction_type: U256::from(2),
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from,
				to: match t.action {
					TransactionAction::Call(to) => Some(to),
					TransactionAction::Create => None,
				},
				value: t.value,
				gas: t.gas_limit,
				// If transaction is not mined yet, gas price is considered just max fee per gas.
				gas_price: Some(t.max_fee_per_gas),
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				input: Bytes(t.input.clone()),
				creates: None,
				chain_id: Some(U64::from(t.chain_id)),
				access_list: Some(t.access_list.clone()),
				authorization_list: None,
				y_parity: Some(U256::from(t.signature.odd_y_parity() as u8)),
				v: Some(U256::from(t.signature.odd_y_parity() as u8)),
				r: U256::from_big_endian(t.signature.r().as_bytes()),
				s: U256::from_big_endian(t.signature.s().as_bytes()),
			},
			EthereumTransaction::EIP7702(t) => Self {
				transaction_type: U256::from(4),
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from,
				to: match t.destination {
					TransactionAction::Call(to) => Some(to),
					TransactionAction::Create => None,
				},
				value: t.value,
				gas: t.gas_limit,
				gas_price: Some(t.max_fee_per_gas),
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				input: Bytes(t.data.clone()),
				creates: None,
				chain_id: Some(U64::from(t.chain_id)),
				access_list: Some(t.access_list.clone()),
				authorization_list: Some(
					t.authorization_list
						.iter()
						.map(|item| AuthorizationListItem {
							address: item.address,
							chain_id: U64::from(item.chain_id),
							nonce: item.nonce,
							y_parity: U64::from(item.signature.odd_y_parity as u8),
							r: U256::from_big_endian(&item.signature.r[..]),
							s: U256::from_big_endian(&item.signature.s[..]),
						})
						.collect(),
				),
				y_parity: Some(U256::from(t.signature.odd_y_parity() as u8)),
				v: Some(U256::from(t.signature.odd_y_parity() as u8)),
				r: U256::from_big_endian(t.signature.r().as_bytes()),
				s: U256::from_big_endian(t.signature.s().as_bytes()),
			},
		}
	}
}

/// Local Transaction Status
#[derive(Debug)]
pub enum LocalTransactionStatus {
	/// Transaction is pending
	Pending,
	/// Transaction is in future part of the queue
	Future,
	/// Transaction was mined.
	Mined(Transaction),
	/// Transaction was removed from the queue, but not mined.
	Culled(Transaction),
	/// Transaction was dropped because of limit.
	Dropped(Transaction),
	/// Transaction was replaced by transaction with higher gas price.
	Replaced(Transaction, U256, H256),
	/// Transaction never got into the queue.
	Rejected(Transaction, String),
	/// Transaction is invalid.
	Invalid(Transaction),
	/// Transaction was canceled.
	Canceled(Transaction),
}

impl Serialize for LocalTransactionStatus {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		use self::LocalTransactionStatus::*;

		let elems = match *self {
			Pending | Future => 1,
			Mined(..) | Culled(..) | Dropped(..) | Invalid(..) | Canceled(..) => 2,
			Rejected(..) => 3,
			Replaced(..) => 4,
		};

		let status = "status";
		let transaction = "transaction";

		let mut struc = serializer.serialize_struct("LocalTransactionStatus", elems)?;
		match *self {
			Pending => struc.serialize_field(status, "pending")?,
			Future => struc.serialize_field(status, "future")?,
			Mined(ref tx) => {
				struc.serialize_field(status, "mined")?;
				struc.serialize_field(transaction, tx)?;
			}
			Culled(ref tx) => {
				struc.serialize_field(status, "culled")?;
				struc.serialize_field(transaction, tx)?;
			}
			Dropped(ref tx) => {
				struc.serialize_field(status, "dropped")?;
				struc.serialize_field(transaction, tx)?;
			}
			Canceled(ref tx) => {
				struc.serialize_field(status, "canceled")?;
				struc.serialize_field(transaction, tx)?;
			}
			Invalid(ref tx) => {
				struc.serialize_field(status, "invalid")?;
				struc.serialize_field(transaction, tx)?;
			}
			Rejected(ref tx, ref reason) => {
				struc.serialize_field(status, "rejected")?;
				struc.serialize_field(transaction, tx)?;
				struc.serialize_field("error", reason)?;
			}
			Replaced(ref tx, ref gas_price, ref hash) => {
				struc.serialize_field(status, "replaced")?;
				struc.serialize_field(transaction, tx)?;
				struc.serialize_field("hash", hash)?;
				struc.serialize_field("gasPrice", gas_price)?;
			}
		}

		struc.end()
	}
}

/// Geth-compatible output for eth_signTransaction method
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RichRawTransaction {
	/// Raw transaction RLP
	pub raw: Bytes,
	/// Transaction details
	#[serde(rename = "tx")]
	pub transaction: Transaction,
}
