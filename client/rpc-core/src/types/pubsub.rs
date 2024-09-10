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

//! Pub-Sub types.

use std::collections::BTreeMap;

use ethereum::{
	BlockV2 as EthereumBlock, ReceiptV3 as EthereumReceipt, TransactionV2 as EthereumTransaction,
};
use ethereum_types::{H256, U256};
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{from_value, Value};
// Substrate
use sp_crypto_hashing::keccak_256;

use crate::types::{Bytes, Filter, FilteredParams, Header, Log, Rich, RichHeader};

/// Subscription kind.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
	/// New block headers subscription.
	NewHeads,
	/// Logs subscription.
	Logs,
	/// New Pending Transactions subscription.
	NewPendingTransactions,
	/// Node syncing status subscription.
	Syncing,
}

/// Subscription kind.
#[derive(Clone, Debug, Eq, PartialEq, Default, Hash)]
pub enum Params {
	/// No parameters passed.
	#[default]
	None,
	/// Log parameters.
	Logs(Filter),
}

impl<'a> Deserialize<'a> for Params {
	fn deserialize<D>(deserializer: D) -> Result<Params, D::Error>
	where
		D: Deserializer<'a>,
	{
		let v: Value = Deserialize::deserialize(deserializer)?;

		if v.is_null() {
			return Ok(Params::None);
		}

		from_value(v)
			.map(Params::Logs)
			.map_err(|e| D::Error::custom(format!("Invalid Pub-Sub parameters: {}", e)))
	}
}

/// Subscription result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PubSubResult {
	/// New block header.
	Header(Box<RichHeader>),
	/// Log
	Log(Box<Log>),
	/// Transaction hash
	TransactionHash(H256),
	/// SyncStatus
	SyncingStatus(PubSubSyncing),
}

impl PubSubResult {
	pub fn header(block: EthereumBlock) -> Self {
		Self::Header(Box::new(Rich {
			inner: Header {
				hash: Some(H256::from(keccak_256(&rlp::encode(&block.header)))),
				parent_hash: block.header.parent_hash,
				uncles_hash: block.header.ommers_hash,
				author: block.header.beneficiary,
				miner: Some(block.header.beneficiary),
				state_root: block.header.state_root,
				transactions_root: block.header.transactions_root,
				receipts_root: block.header.receipts_root,
				number: Some(block.header.number),
				gas_used: block.header.gas_used,
				gas_limit: block.header.gas_limit,
				extra_data: Bytes(block.header.extra_data.clone()),
				logs_bloom: block.header.logs_bloom,
				timestamp: U256::from(block.header.timestamp),
				difficulty: block.header.difficulty,
				nonce: Some(block.header.nonce),
				size: Some(U256::from(rlp::encode(&block.header).len() as u32)),
			},
			extra_info: BTreeMap::new(),
		}))
	}

	pub fn logs(
		block: EthereumBlock,
		receipts: Vec<EthereumReceipt>,
		params: &FilteredParams,
	) -> impl Iterator<Item = Self> {
		let block_number = block.header.number;
		let block_hash = block.header.hash();

		let mut logs: Vec<Log> = vec![];
		let mut log_index: u32 = 0;
		for (receipt_index, receipt) in receipts.into_iter().enumerate() {
			let receipt_logs = match receipt {
				EthereumReceipt::Legacy(d)
				| EthereumReceipt::EIP2930(d)
				| EthereumReceipt::EIP1559(d) => d.logs,
			};

			let transaction_hash: Option<H256> = if !receipt_logs.is_empty() {
				Some(block.transactions[receipt_index].hash())
			} else {
				None
			};

			let mut transaction_log_index = 0;
			for log in receipt_logs {
				if params.is_not_filtered(block_number, block_hash, &log.address, &log.topics) {
					logs.push(Log {
						address: log.address,
						topics: log.topics,
						data: Bytes(log.data),
						block_hash: Some(block_hash),
						block_number: Some(block_number),
						transaction_hash,
						transaction_index: Some(U256::from(receipt_index)),
						log_index: Some(U256::from(log_index)),
						transaction_log_index: Some(U256::from(transaction_log_index)),
						removed: false,
					});
				}
				transaction_log_index += 1;
				log_index += 1;
			}
		}
		logs.into_iter().map(|log| Self::Log(Box::new(log)))
	}

	pub fn transaction_hash(tx: &EthereumTransaction) -> Self {
		Self::TransactionHash(tx.hash())
	}
}

impl Serialize for PubSubResult {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			Self::Header(ref header) => header.serialize(serializer),
			Self::Log(ref log) => log.serialize(serializer),
			Self::TransactionHash(ref hash) => hash.serialize(serializer),
			Self::SyncingStatus(ref sync) => sync.serialize(serializer),
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PubSubSyncing {
	Synced(bool),
	Syncing(SyncingStatus),
}

/// Pubsub sync status
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncingStatus {
	pub starting_block: u64,
	pub current_block: u64,
	#[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
	pub highest_block: Option<u64>,
}
