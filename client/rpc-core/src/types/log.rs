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

use ethereum_types::{H160, H256, U256};
use serde::Serialize;

use crate::types::Bytes;

/// Log
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
	/// H160
	pub address: H160,
	/// Topics
	pub topics: Vec<H256>,
	/// Data
	pub data: Bytes,
	/// Block Hash
	pub block_hash: Option<H256>,
	/// Block Number
	pub block_number: Option<U256>,
	/// Transaction Hash
	pub transaction_hash: Option<H256>,
	/// Transaction Index
	pub transaction_index: Option<U256>,
	/// Log Index in Block
	pub log_index: Option<U256>,
	/// Log Index in Transaction
	pub transaction_log_index: Option<U256>,
	/// Whether Log Type is Removed (Geth Compatibility Field)
	#[serde(default)]
	pub removed: bool,
}
