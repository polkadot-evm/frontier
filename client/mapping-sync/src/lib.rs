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

#![warn(unused_crate_dependencies)]
#![allow(clippy::too_many_arguments)]

pub mod kv;
#[cfg(feature = "sql")]
pub mod sql;

use sp_blockchain::TreeRoute;
use sp_consensus::SyncOracle;
use sp_runtime::traits::Block as BlockT;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum SyncStrategy {
	Normal,
	Parachain,
}

pub type EthereumBlockNotificationSinks<T> =
	parking_lot::Mutex<Vec<sc_utils::mpsc::TracingUnboundedSender<T>>>;

/// Information about a chain reorganization.
///
/// When a reorg occurs, this struct contains the blocks that were removed from
/// the canonical chain (retracted) and the blocks that were added (enacted).
/// The `common_ancestor` is the last block that remains canonical in both
/// the old and new chains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorgInfo<Block: BlockT> {
	/// The common ancestor block hash between the old and new canonical chains.
	pub common_ancestor: Block::Hash,
	/// Blocks that were removed from the canonical chain (old fork).
	pub retracted: Vec<Block::Hash>,
	/// Blocks that were added to the canonical chain (new fork).
	pub enacted: Vec<Block::Hash>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EthereumBlockNotification<Block: BlockT> {
	pub is_new_best: bool,
	pub hash: Block::Hash,
	/// Optional reorg information. Present when this block became best as part of a reorg.
	pub reorg_info: Option<ReorgInfo<Block>>,
}

/// Extract reorg information from a tree route.
pub fn extract_reorg_info<Block: BlockT>(
	tree_route: &TreeRoute<Block>,
	new_best_hash: Block::Hash,
) -> ReorgInfo<Block> {
	let retracted = tree_route
		.retracted()
		.iter()
		.map(|hash_and_number| hash_and_number.hash)
		.collect();

	// tree_route is "from old best to new best parent", so enacted() excludes
	// the new best block itself. We append it manually, with a defensive check
	// in case the TreeRoute implementation changes in the future.
	let mut enacted: Vec<_> = tree_route
		.enacted()
		.iter()
		.map(|hash_and_number| hash_and_number.hash)
		.collect();

	if enacted.last() != Some(&new_best_hash) {
		enacted.push(new_best_hash);
	}

	ReorgInfo {
		common_ancestor: tree_route.common_block().hash,
		retracted,
		enacted,
	}
}

/// Context for emitting block notifications.
/// Contains all information needed to emit a notification consistently
/// across both KV and SQL backends.
pub struct BlockNotificationContext<Block: BlockT> {
	/// The block hash being notified about.
	pub hash: Block::Hash,
	/// Whether this block is the new best block.
	pub is_new_best: bool,
	/// Optional reorg information if this block became best as part of a reorg.
	pub reorg_info: Option<ReorgInfo<Block>>,
}

/// Emit block notification to all registered sinks.
///
/// This function provides a unified notification mechanism for both KV and SQL backends:
/// - Clears all sinks when major syncing (to prevent stale subscriptions)
/// - Sends notification to all sinks and removes closed sinks when not syncing
///
/// Both backends should call this function after completing block sync/indexing
/// to ensure consistent notification behavior regardless of the storage backend used.
pub fn emit_block_notification<Block: BlockT>(
	pubsub_notification_sinks: &EthereumBlockNotificationSinks<EthereumBlockNotification<Block>>,
	sync_oracle: &dyn SyncOracle,
	context: BlockNotificationContext<Block>,
) {
	let sinks = &mut pubsub_notification_sinks.lock();

	if sync_oracle.is_major_syncing() {
		// Remove all sinks when major syncing to prevent stale subscriptions
		sinks.clear();
		return;
	}

	sinks.retain(|sink| {
		sink.unbounded_send(EthereumBlockNotification {
			is_new_best: context.is_new_best,
			hash: context.hash,
			reorg_info: context.reorg_info.clone(),
		})
		.is_ok()
	});
}
