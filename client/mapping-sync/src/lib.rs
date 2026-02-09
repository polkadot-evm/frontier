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
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum SyncStrategy {
	Normal,
	Parachain,
}

pub type EthereumBlockNotificationSinks<T> =
	parking_lot::Mutex<Vec<sc_utils::mpsc::TracingUnboundedSender<T>>>;

/// Default hard cap for pending notifications per subscriber channel.
/// Subscribers above this threshold are considered lagging and are dropped.
const DEFAULT_MAX_PENDING_NOTIFICATIONS_PER_SUBSCRIBER: usize = 512;

static MAX_PENDING_NOTIFICATIONS_PER_SUBSCRIBER: AtomicUsize =
	AtomicUsize::new(DEFAULT_MAX_PENDING_NOTIFICATIONS_PER_SUBSCRIBER);

/// Configure the hard cap for pending notifications per subscriber channel.
pub fn set_max_pending_notifications_per_subscriber(max_pending: usize) {
	MAX_PENDING_NOTIFICATIONS_PER_SUBSCRIBER.store(max_pending.max(1), Ordering::Relaxed);
}

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
	/// Blocks that were added to the canonical chain (new fork), excluding `new_best`.
	pub enacted: Vec<Block::Hash>,
	/// The new best block hash that triggered this reorg.
	pub new_best: Block::Hash,
}

impl<Block: BlockT> ReorgInfo<Block> {
	/// Create reorg info from a tree route and the new best block hash.
	///
	/// `tree_route` is "from old best to new best parent", so `enacted()` excludes
	/// the new best block itself. The `new_best` is stored separately and callers
	/// should handle emitting it after the enacted blocks.
	pub fn from_tree_route(tree_route: &TreeRoute<Block>, new_best: Block::Hash) -> Self {
		let retracted = tree_route
			.retracted()
			.iter()
			.map(|hash_and_number| hash_and_number.hash)
			.collect();

		let enacted = tree_route
			.enacted()
			.iter()
			.map(|hash_and_number| hash_and_number.hash)
			.collect();

		Self {
			common_ancestor: tree_route.common_block().hash,
			retracted,
			enacted,
			new_best,
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EthereumBlockNotification<Block: BlockT> {
	pub is_new_best: bool,
	pub hash: Block::Hash,
	/// Optional reorg information. Present when this block became best as part of a reorg.
	pub reorg_info: Option<Arc<ReorgInfo<Block>>>,
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
	pub reorg_info: Option<Arc<ReorgInfo<Block>>>,
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
		let max_pending = MAX_PENDING_NOTIFICATIONS_PER_SUBSCRIBER.load(Ordering::Relaxed);
		if sink.len() >= max_pending {
			log::debug!(
				target: "mapping-sync",
				"Dropping lagging pubsub subscriber (pending={}, max={})",
				sink.len(),
				max_pending,
			);
			let _ = sink.close();
			return false;
		}

		sink.unbounded_send(EthereumBlockNotification {
			is_new_best: context.is_new_best,
			hash: context.hash,
			reorg_info: context.reorg_info.clone(),
		})
		.is_ok()
	});
}
