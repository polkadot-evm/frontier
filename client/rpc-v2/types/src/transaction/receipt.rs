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

use crate::{log::Log, transaction::TxType};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionReceipt {
	/// Hash of the block this transaction was included within.
	pub block_hash: Option<H256>,
	/// Number of the block this transaction was included within.
	pub block_number: Option<U64>,

	/// Transaction hash.
	pub transaction_hash: H256,
	/// Transaction index within the block.
	pub transaction_index: U64,
	#[serde(rename = "type")]
	pub tx_type: TxType,
	/// Gas used by this transaction.
	pub gas_used: U64,

	/// Address of the sender
	pub from: Address,
	/// Address of the receiver, or None when it's a contract creation transaction.
	pub to: Option<Address>,
	/// Contract address created, or None if not a deployment.
	pub contract_address: Option<Address>,

	/// The price paid post-execution by the transaction.
	/// Pre-eip1559 : gas price.
	/// Post-eip1559: base fee + priority fee.
	pub effective_gas_price: U256,

	/// Transaction execution status.
	pub status: U64,

	/// Cumulative gas used.
	pub cumulative_gas_used: U64,

	/// Log send from contracts.
	pub logs: Vec<Log>,
	/// [`Log`]'s bloom filter
	pub logs_bloom: Bloom,

	/// The post-transaction state root (pre Byzantium).
	#[serde(rename = "root", skip_serializing_if = "Option::is_none")]
	pub state_root: Option<H256>,
}
