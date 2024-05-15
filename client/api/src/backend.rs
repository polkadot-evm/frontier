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

use scale_codec::{Decode, Encode};
// Substrate
use sp_core::{H160, H256};
use sp_runtime::traits::Block as BlockT;
// Frontier
use fp_storage::EthereumStorageSchema;

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
pub struct TransactionMetadata<Block: BlockT> {
	pub substrate_block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub ethereum_index: u32,
}

/// The frontier backend interface.
#[async_trait::async_trait]
pub trait Backend<Block: BlockT>: Send + Sync {
	/// Get the substrate hash with the given ethereum block hash.
	async fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String>;

	/// Get the transaction metadata with the given ethereum block hash.
	async fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String>;

	/// Returns reference to log indexer backend.
	fn log_indexer(&self) -> &dyn LogIndexerBackend<Block>;

	/// Indicate whether the log indexing feature is supported.
	fn is_indexed(&self) -> bool {
		self.log_indexer().is_indexed()
	}

	/// Get the hash of the latest substrate block fully indexed by the backend.
	async fn latest_block_hash(&self) -> Result<Block::Hash, String>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct FilteredLog<Block: BlockT> {
	pub substrate_block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub block_number: u32,
	pub ethereum_storage_schema: EthereumStorageSchema,
	pub transaction_index: u32,
	pub log_index: u32,
}

/// The log indexer backend interface.
#[async_trait::async_trait]
pub trait LogIndexerBackend<Block: BlockT>: Send + Sync {
	/// Indicate whether the log indexing feature is supported.
	fn is_indexed(&self) -> bool;

	/// Filter the logs by the parameters.
	async fn filter_logs(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Vec<H160>,
		topics: Vec<Vec<Option<H256>>>,
	) -> Result<Vec<FilteredLog<Block>>, String>;
}
