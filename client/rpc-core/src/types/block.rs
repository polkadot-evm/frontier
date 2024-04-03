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

use std::{collections::BTreeMap, ops::Deref};

use ethereum_types::{Bloom as H2048, H160, H256, H64, U256};
use serde::{ser::Error, Serialize, Serializer};

use crate::types::{Bytes, Transaction};

/// Block Transactions
#[derive(Clone, Debug)]
pub enum BlockTransactions {
	/// Only hashes
	Hashes(Vec<H256>),
	/// Full transactions
	Full(Vec<Transaction>),
}

impl Serialize for BlockTransactions {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			BlockTransactions::Hashes(ref hashes) => hashes.serialize(serializer),
			BlockTransactions::Full(ref ts) => ts.serialize(serializer),
		}
	}
}

/// Block representation
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
	/// Header of the block
	#[serde(flatten)]
	pub header: Header,
	/// Total difficulty
	pub total_difficulty: Option<U256>,
	/// Uncles' hashes
	pub uncles: Vec<H256>,
	/// Transactions
	pub transactions: BlockTransactions,
	/// Size in bytes
	pub size: Option<U256>,
	/// Base Fee for post-EIP1559 blocks.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub base_fee_per_gas: Option<U256>,
}

/// Block header representation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
	/// Hash of the block
	pub hash: Option<H256>,
	/// Hash of the parent
	pub parent_hash: H256,
	/// Hash of the uncles
	#[serde(rename = "sha3Uncles")]
	pub uncles_hash: H256,
	/// Authors address
	pub author: H160,
	/// Alias of `author`
	pub miner: Option<H160>,
	/// State root hash
	pub state_root: H256,
	/// Transactions root hash
	pub transactions_root: H256,
	/// Transactions receipts root hash
	pub receipts_root: H256,
	/// Block number
	pub number: Option<U256>,
	/// Gas Used
	pub gas_used: U256,
	/// Gas Limit
	pub gas_limit: U256,
	/// Extra data
	pub extra_data: Bytes,
	/// Logs bloom
	pub logs_bloom: H2048,
	/// Timestamp
	pub timestamp: U256,
	/// Difficulty
	pub difficulty: U256,
	/// Nonce
	pub nonce: Option<H64>,
	/// Size in bytes
	pub size: Option<U256>,
}

/// Block representation with additional info.
pub type RichBlock = Rich<Block>;

/// Header representation with additional info.
pub type RichHeader = Rich<Header>;

/// Value representation with additional info
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rich<T> {
	/// Standard value.
	pub inner: T,
	/// Engine-specific fields with additional description.
	/// Should be included directly to serialized block object.
	// TODO [ToDr] #[serde(skip_serializing)]
	pub extra_info: BTreeMap<String, String>,
}

impl<T> Deref for Rich<T> {
	type Target = T;
	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<T: Serialize> Serialize for Rich<T> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		use serde_json::{to_value, Value};

		let serialized = (to_value(&self.inner), to_value(&self.extra_info));
		if let (Ok(Value::Object(mut value)), Ok(Value::Object(extras))) = serialized {
			// join two objects
			value.extend(extras);
			// and serialize
			value.serialize(serializer)
		} else {
			Err(S::Error::custom(
				"Unserializable structures: expected objects",
			))
		}
	}
}
