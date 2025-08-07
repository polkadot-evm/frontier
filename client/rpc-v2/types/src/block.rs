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

use ethereum_types::{Address, Bloom, H256, U256, U64};
use serde::{Deserialize, Serialize};

use crate::{bytes::Bytes, transaction::Transaction};

/// Block information.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
	/// Block header.
	#[serde(flatten)]
	pub header: Header,

	/// Block transactions.
	pub transactions: BlockTransactions,

	/// Uncles' hashes.
	#[serde(default)]
	pub uncles: Vec<H256>,

	/// Block size in bytes.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub size: Option<U256>,

	/// Withdrawals, see [EIP-4895](https://eips.ethereum.org/EIPS/eip-4895).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub withdrawals: Option<Vec<Withdrawal>>,
}

/// Block header representation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
	/// Block number.
	pub number: U256,
	/// Block hash.
	pub hash: Option<H256>,
	/// Hash of the parent block.
	pub parent_hash: H256,
	/// Hash of the uncles.
	#[serde(rename = "sha3Uncles")]
	pub uncles_hash: H256,
	/// Nonce.
	pub nonce: Option<U64>,
	/// Authors address.
	#[serde(rename = "miner")]
	pub author: Address,
	/// State root hash.
	pub state_root: H256,
	/// Transactions root hash.
	pub transactions_root: H256,
	/// Transactions receipts root hash.
	pub receipts_root: H256,
	/// Logs bloom.
	pub logs_bloom: Bloom,
	/// Gas limit.
	pub gas_limit: U256,
	/// Gas used
	pub gas_used: U256,
	/// Timestamp.
	pub timestamp: U64,
	/// Extra data.
	pub extra_data: Bytes,
	/// Difficulty.
	pub difficulty: U256,
	/// Total difficulty.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub total_difficulty: Option<U256>,
	/// Mix hash.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub mix_hash: Option<H256>,

	/// Base fee per unit of gas, which is added by [EIP-1559](https://eips.ethereum.org/EIPS/eip-1559).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub base_fee_per_gas: Option<U256>,
	/// Withdrawals root hash, which is added by [EIP-4895](https://eips.ethereum.org/EIPS/eip-4895).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub withdrawals_root: Option<H256>,
	/// Parent beacon block root, which is added by [EIP-4788](https://eips.ethereum.org/EIPS/eip-4788).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub parent_beacon_block_root: Option<H256>,
}

/// Block Transactions
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlockTransactions {
	/// Only hashes.
	Hashes(Vec<H256>),
	/// Full transactions.
	Full(Vec<Transaction>),
}

// Withdrawal represents a validator withdrawal from the consensus layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Withdrawal {
	/// Monotonically increasing identifier issued by consensus layer.
	pub index: U64,
	/// Index of validator associated with withdrawal.
	pub validator_index: U64,
	/// Target address for withdrawn ether.
	pub address: Address,
	/// Value of withdrawal in Gwei.
	pub amount: U64,
}

/// [`BlockOverrides`] is a set of header fields to override.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BlockOverrides {
	/// Fake block number.
	// Note: geth uses `number`, erigon uses `blockNumber`
	#[serde(
		default,
		skip_serializing_if = "Option::is_none",
		alias = "blockNumber"
	)]
	pub number: Option<U256>,
	/// Fake difficulty.
	// Note post-merge difficulty should be 0.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub difficulty: Option<U256>,
	/// Fake block timestamp.
	// Note: geth uses `time`, erigon uses `timestamp`
	#[serde(default, skip_serializing_if = "Option::is_none", alias = "timestamp")]
	pub time: Option<U64>,
	/// Block gas capacity.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gas_limit: Option<U64>,
	/// Block fee recipient.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub coinbase: Option<Address>,
	/// Fake PrevRandao value.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub random: Option<H256>,
	/// Block base fee.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub base_fee: Option<U256>,
}
