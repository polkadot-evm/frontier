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

//! Return types for RPC calls

use ethereum_types::{Address, Public, H160, H256, U256};
use serde::Serialize;

use crate::types::Bytes;

/// Account information.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AccountInfo {
	/// Account name
	pub name: String,
}

/// Data structure with proof for one single storage-entry
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageProof {
	pub key: U256,
	pub value: U256,
	pub proof: Vec<Bytes>,
}

/// Account information.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EthAccount {
	pub address: H160,
	pub balance: U256,
	pub nonce: U256,
	pub code_hash: H256,
	pub storage_hash: H256,
	pub account_proof: Vec<Bytes>,
	pub storage_proof: Vec<StorageProof>,
}

/// Extended account information (used by `parity_allAccountInfo`).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ExtAccountInfo {
	/// Account name
	pub name: String,
	/// Account meta JSON
	pub meta: String,
	/// Account UUID (`None` for address book entries)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub uuid: Option<String>,
}

/// account derived from a signature
/// as well as information that tells if it is valid for
/// the current chain
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveredAccount {
	/// address of the recovered account
	pub address: Address,
	/// public key of the recovered account
	pub public_key: Public,
	/// If the signature contains chain replay protection,
	/// And the chain_id encoded within the signature
	/// matches the current chain this would be true, otherwise false.
	pub is_valid_for_current_chain: bool,
}
