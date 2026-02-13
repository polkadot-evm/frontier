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

use sp_blockchain::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto};

use crate::ReorgInfo;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconcileWindow {
	pub start: u64,
	pub end: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconcileStats {
	pub scanned: u64,
	pub updated: u64,
	pub first_unresolved: Option<u64>,
	pub highest_reconciled: Option<u64>,
	pub next_cursor: u64,
	pub lag_blocks: u64,
	pub window: ReconcileWindow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CursorUpdateStrategy {
	Replace,
	KeepLower,
}

pub fn build_reconcile_window<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	reorg_info: Option<&ReorgInfo<Block>>,
	new_best_hash: Block::Hash,
) -> Result<Option<ReconcileWindow>, String> {
	let Some(new_best_header) = client.header(new_best_hash).map_err(|e| format!("{e:?}"))? else {
		return Ok(None);
	};
	let end: u64 = (*new_best_header.number()).unique_saturated_into();
	let mut start = end;

	if let Some(info) = reorg_info {
		if let Some(common_header) = client
			.header(info.common_ancestor)
			.map_err(|e| format!("{e:?}"))?
		{
			let common_number: u64 = (*common_header.number()).unique_saturated_into();
			start = start.min(common_number.saturating_add(1));
		}

		for hash in info.enacted.iter().chain(info.retracted.iter()) {
			if let Some(header) = client.header(*hash).map_err(|e| format!("{e:?}"))? {
				let number: u64 = (*header.number()).unique_saturated_into();
				start = start.min(number);
			}
		}
	}

	Ok(Some(ReconcileWindow {
		start: start.min(end),
		end,
	}))
}

pub fn reconcile_reorg_window<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn fc_storage::StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	reorg_info: Option<&ReorgInfo<Block>>,
	new_best_hash: Block::Hash,
	sync_from: <Block::Header as HeaderT>::Number,
) -> Result<Option<ReconcileStats>, String> {
	let Some(window) = build_reconcile_window(client, reorg_info, new_best_hash)? else {
		return Ok(None);
	};

	let sync_from_number = UniqueSaturatedInto::<u64>::unique_saturated_into(sync_from);
	let stats = reconcile_range_internal(
		client,
		storage_override,
		frontier_backend,
		window.start,
		window.end,
		sync_from_number,
		CursorUpdateStrategy::KeepLower,
	)?;
	Ok(Some(stats))
}

pub fn reconcile_from_cursor_batch<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn fc_storage::StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	sync_from: <Block::Header as HeaderT>::Number,
	max_blocks: u64,
) -> Result<Option<ReconcileStats>, String> {
	if max_blocks == 0 {
		return Ok(None);
	}

	let best_number: u64 = client.info().best_number.unique_saturated_into();
	let sync_from_number = UniqueSaturatedInto::<u64>::unique_saturated_into(sync_from);
	let start = frontier_backend
		.mapping()
		.canonical_number_repair_cursor()?
		.unwrap_or(sync_from_number)
		.max(sync_from_number)
		.min(best_number);
	let end = start
		.saturating_add(max_blocks.saturating_sub(1))
		.min(best_number);

	let stats = reconcile_range_internal(
		client,
		storage_override,
		frontier_backend,
		start,
		end,
		sync_from_number,
		CursorUpdateStrategy::Replace,
	)?;
	Ok(Some(stats))
}

fn reconcile_range_internal<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn fc_storage::StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	start: u64,
	end: u64,
	sync_from_number: u64,
	cursor_update: CursorUpdateStrategy,
) -> Result<ReconcileStats, String> {
	if end < start {
		let lag_blocks = compute_lag_blocks(client, frontier_backend)?;
		return Ok(ReconcileStats {
			scanned: 0,
			updated: 0,
			first_unresolved: None,
			highest_reconciled: None,
			next_cursor: sync_from_number,
			lag_blocks,
			window: ReconcileWindow { start, end },
		});
	}

	let best_number: u64 = client.info().best_number.unique_saturated_into();
	let mut updated = 0u64;
	let mut first_unresolved = None;
	let mut highest_reconciled = None;

	for number in start..=end {
		let Some(canonical_hash) = client
			.hash(number.unique_saturated_into())
			.map_err(|e| format!("{e:?}"))?
		else {
			first_unresolved.get_or_insert(number);
			continue;
		};
		let Some(ethereum_block) = storage_override.current_block(canonical_hash) else {
			first_unresolved.get_or_insert(number);
			continue;
		};
		let canonical_eth_hash = ethereum_block.header.hash();
		let should_update =
			frontier_backend.mapping().block_hash_by_number(number)? != Some(canonical_eth_hash);
		if should_update {
			frontier_backend
				.mapping()
				.set_block_hash_by_number(number, canonical_eth_hash)?;
			updated = updated.saturating_add(1);
		}
		highest_reconciled = Some(number);
	}

	let next_cursor = if let Some(unresolved) = first_unresolved {
		unresolved
	} else if end >= best_number {
		best_number
	} else {
		end.saturating_add(1)
	};
	update_repair_cursor(
		frontier_backend,
		sync_from_number,
		next_cursor,
		cursor_update,
	)?;

	if let Some(number) = highest_reconciled {
		advance_latest_pointer(frontier_backend, number)?;
	}

	validate_latest_pointer_invariant(client, storage_override, frontier_backend)?;

	let scanned = end.saturating_sub(start).saturating_add(1);
	let lag_blocks = compute_lag_blocks(client, frontier_backend)?;
	let stats = ReconcileStats {
		scanned,
		updated,
		first_unresolved,
		highest_reconciled,
		next_cursor,
		lag_blocks,
		window: ReconcileWindow { start, end },
	};

	log::debug!(
		target: "reconcile",
		"reconcile range #{}..#{}, scanned {}, updated {}, first_unresolved {:?}, highest_reconciled {:?}, next_cursor #{}, frontier_reconcile_lag_blocks {}",
		stats.window.start,
		stats.window.end,
		stats.scanned,
		stats.updated,
		stats.first_unresolved,
		stats.highest_reconciled,
		stats.next_cursor,
		stats.lag_blocks,
	);

	Ok(stats)
}

fn update_repair_cursor<Block: BlockT, C: HeaderBackend<Block>>(
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	sync_from_number: u64,
	candidate_next: u64,
	strategy: CursorUpdateStrategy,
) -> Result<(), String> {
	let candidate_next = candidate_next.max(sync_from_number);
	let current = frontier_backend
		.mapping()
		.canonical_number_repair_cursor()?;

	let next = match (strategy, current) {
		(CursorUpdateStrategy::Replace, _) => candidate_next,
		(CursorUpdateStrategy::KeepLower, Some(current)) => current.min(candidate_next),
		(CursorUpdateStrategy::KeepLower, None) => candidate_next,
	};

	frontier_backend
		.mapping()
		.set_canonical_number_repair_cursor(next)
}

pub fn advance_latest_pointer<Block: BlockT, C: HeaderBackend<Block>>(
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	block_number: u64,
) -> Result<(), String> {
	let latest_indexed = frontier_backend
		.mapping()
		.latest_canonical_indexed_block_number()?;
	if latest_indexed.is_none_or(|current| block_number > current) {
		frontier_backend
			.mapping()
			.set_latest_canonical_indexed_block(block_number)?;
	}
	Ok(())
}

fn compute_lag_blocks<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
) -> Result<u64, String> {
	let best_number: u64 = client.info().best_number.unique_saturated_into();
	let latest_indexed = frontier_backend
		.mapping()
		.latest_canonical_indexed_block_number()?
		.unwrap_or(0);
	Ok(best_number.saturating_sub(latest_indexed))
}

fn validate_latest_pointer_invariant<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn fc_storage::StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
) -> Result<(), String> {
	let Some(latest_indexed) = frontier_backend
		.mapping()
		.latest_canonical_indexed_block_number()?
	else {
		return Ok(());
	};

	let Some(canonical_hash) = client
		.hash(latest_indexed.unique_saturated_into())
		.map_err(|e| format!("{e:?}"))?
	else {
		return Ok(());
	};
	let Some(canonical_eth_hash) = storage_override
		.current_block(canonical_hash)
		.map(|block| block.header.hash())
	else {
		return Ok(());
	};
	if frontier_backend
		.mapping()
		.block_hash_by_number(latest_indexed)?
		!= Some(canonical_eth_hash)
	{
		log::warn!(
			target: "reconcile",
			"invariant mismatch at latest pointer #{latest_indexed}: expected {canonical_eth_hash:?}",
		);
	}

	Ok(())
}
