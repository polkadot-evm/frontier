// This file is part of Tokfin.

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

use ethereum_types::{Address, H256, U256, U64};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;

/// The response type of `eth_getProof`.
///
/// See [EIP-1186](https://eips.ethereum.org/EIPS/eip-1186) for more details.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProof {
	/// The address of the account.
	pub address: Address,
	/// Array of rlp-serialized MerkleTree-Nodes, starting with the stateRoot-Node, following the
	/// path of the SHA3 (address) as key.
	pub account_proof: Vec<Bytes>,
	/// The balance of the account.
	pub balance: U256,
	/// Code hash of the account.
	pub code_hash: H256,
	/// The nonce of the account.
	pub nonce: U64,
	/// The hash of storage root.
	pub storage_hash: H256,
	/// Array of storage-entries as requested.
	pub storage_proof: Vec<StorageProof>,
}

/// Data structure with proof for one single storage-entry
#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageProof {
	/// Storage key.
	pub key: H256,
	/// Storage value.
	pub value: U256,
	/// Array of rlp-serialized MerkleTree-Nodes, starting with the storageHash-Node, following the
	/// path of the SHA3 (key) as path.
	pub proof: Vec<Bytes>,
}
