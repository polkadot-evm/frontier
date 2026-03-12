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

#![allow(clippy::too_many_arguments)]

mod canonical_reconciler;
mod worker;

pub use worker::MappingSyncWorker;

use std::{collections::HashMap, sync::Arc};

// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::{Backend as _, HeaderBackend};
use sp_consensus::SyncOracle;
use sp_runtime::traits::{
	Block as BlockT, Header as HeaderT, SaturatedConversion, UniqueSaturatedInto, Zero,
};
// Frontier
use fc_storage::StorageOverride;
use fp_consensus::{FindLogError, Hashes, Log, PostLog, PreLog};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	emit_block_notification, BlockNotificationContext, EthereumBlockNotification,
	EthereumBlockNotificationSinks, SyncStrategy,
};
use worker::BestBlockInfo;

pub const CANONICAL_NUMBER_REPAIR_BATCH_SIZE: u64 = 2048;

/// Max blocks to backfill in one skip-path call to avoid unbounded stall on heavily pruned nodes.
const BACKFILL_ON_SKIP_MAX_BLOCKS: u64 = 1024;

/// Number of recent blocks to reconcile on every mapping-sync worker tick.
/// Small enough to be cheap per-tick, large enough to cover typical reorg depth.
pub const PERIODIC_RECONCILE_WINDOW: u64 = 16;

/// Max blocks to repair per idle tick via the cursor-driven full-history sweep.
/// Keeps per-tick cost bounded while ensuring eventual consistency across the entire chain.
pub const CURSOR_REPAIR_IDLE_BATCH: u64 = 128;

/// Sync a single block's Ethereum mapping from its consensus digest into the Frontier DB.
pub fn sync_block<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: Arc<dyn StorageOverride<Block>>,
	backend: &fc_db::kv::Backend<Block, C>,
	header: &Block::Header,
) -> Result<(), String> {
	let substrate_block_hash = header.hash();
	let block_number: u64 = (*header.number()).unique_saturated_into();

	// Write BLOCK_NUMBER_MAPPING when this block is canonical at this number, so
	// latest_block_hash() / indexed_canonical_hash_at() find it during catch-up.
	// Uses only HeaderBackend::hash() — no state access, pruning-safe.
	// Ok(None) (block number unknown) falls back to Skip; Err is propagated so
	// the block stays unsynced and fetch_header retries it on the next tick.
	let canonical_hash_at_number = client
		.hash(*header.number())
		.map_err(|e| format!("failed to resolve canonical hash at #{block_number}: {e:?}"))?;
	let number_mapping_write = if canonical_hash_at_number == Some(substrate_block_hash) {
		fc_db::kv::NumberMappingWrite::Write
	} else {
		fc_db::kv::NumberMappingWrite::Skip
	};

	match fp_consensus::find_log(header.digest()) {
		Ok(log) => {
			let gen_from_hashes = |hashes: Hashes| -> fc_db::kv::MappingCommitment<Block> {
				fc_db::kv::MappingCommitment {
					block_hash: substrate_block_hash,
					ethereum_block_hash: hashes.block_hash,
					ethereum_transaction_hashes: hashes.transaction_hashes,
				}
			};
			let gen_from_block = |block| -> fc_db::kv::MappingCommitment<Block> {
				let hashes = Hashes::from_block(block);
				gen_from_hashes(hashes)
			};

			match log {
				Log::Pre(PreLog::Block(block)) => {
					let mapping_commitment = gen_from_block(block);
					backend.mapping().write_hashes(
						mapping_commitment,
						block_number,
						number_mapping_write,
					)
				}
				Log::Post(post_log) => match post_log {
					PostLog::Hashes(hashes) => {
						let mapping_commitment = gen_from_hashes(hashes);
						backend.mapping().write_hashes(
							mapping_commitment,
							block_number,
							number_mapping_write,
						)
					}
					PostLog::Block(block) => {
						let mapping_commitment = gen_from_block(block);
						backend.mapping().write_hashes(
							mapping_commitment,
							block_number,
							number_mapping_write,
						)
					}
					PostLog::BlockHash(expect_eth_block_hash) => {
						let ethereum_block = storage_override.current_block(substrate_block_hash);
						match ethereum_block {
							Some(block) => {
								let got_eth_block_hash = block.header.hash();
								if got_eth_block_hash != expect_eth_block_hash {
									Err(format!(
										"Ethereum block hash mismatch: \
										frontier consensus digest ({expect_eth_block_hash:?}), \
										db state ({got_eth_block_hash:?})"
									))
								} else {
									let mapping_commitment = gen_from_block(block);
									backend.mapping().write_hashes(
										mapping_commitment,
										block_number,
										number_mapping_write,
									)
								}
							}
							None => {
								// State is unavailable — likely pruned. Write a minimal
								// commitment so BLOCK_MAPPING and BLOCK_NUMBER_MAPPING are
								// populated and indexed_canonical_hash_at() can resolve
								// this block. Transaction hashes are unavailable without state.
								log::warn!(
									target: "mapping-sync",
									"State unavailable for block #{block_number} ({substrate_block_hash:?}); \
									writing minimal mapping (no tx hashes). \
									This may indicate the pruning window is too narrow.",
								);
								let mapping_commitment = fc_db::kv::MappingCommitment::<Block> {
									block_hash: substrate_block_hash,
									ethereum_block_hash: expect_eth_block_hash,
									ethereum_transaction_hashes: vec![],
								};
								backend.mapping().write_hashes(
									mapping_commitment,
									block_number,
									number_mapping_write,
								)
							}
						}
					}
				},
			}
		}
		Err(FindLogError::NotFound) => backend.mapping().write_none(substrate_block_hash),
		Err(FindLogError::MultipleLogs) => Err("Multiple logs found".to_string()),
	}
}

pub fn sync_genesis_block<Block: BlockT, C>(
	client: &C,
	backend: &fc_db::kv::Backend<Block, C>,
	header: &Block::Header,
) -> Result<(), String>
where
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block>,
	C::Api: EthereumRuntimeRPCApi<Block>,
{
	let substrate_block_hash = header.hash();
	let block_number: u64 = (*header.number()).unique_saturated_into();

	if let Some(api_version) = client
		.runtime_api()
		.api_version::<dyn EthereumRuntimeRPCApi<Block>>(substrate_block_hash)
		.map_err(|e| format!("{e:?}"))?
	{
		let block = if api_version > 1 {
			client
				.runtime_api()
				.current_block(substrate_block_hash)
				.map_err(|e| format!("{e:?}"))?
		} else {
			#[allow(deprecated)]
			let legacy_block = client
				.runtime_api()
				.current_block_before_version_2(substrate_block_hash)
				.map_err(|e| format!("{e:?}"))?;
			legacy_block.map(|block| block.into())
		};
		let block_hash = block
			.ok_or_else(|| "Ethereum genesis block not found".to_string())?
			.header
			.hash();
		let mapping_commitment = fc_db::kv::MappingCommitment::<Block> {
			block_hash: substrate_block_hash,
			ethereum_block_hash: block_hash,
			ethereum_transaction_hashes: Vec::new(),
		};
		backend.mapping().write_hashes(
			mapping_commitment,
			block_number,
			fc_db::kv::NumberMappingWrite::Write,
		)?;
	} else {
		backend.mapping().write_none(substrate_block_hash)?;
	};

	Ok(())
}

/// Backfill BLOCK_NUMBER_MAPPING for already-synced canonical blocks in `[from..=to]`.
/// Uses only `HeaderBackend` and consensus digests — no state access, pruning-safe.
/// Stops after writing `max_blocks` mappings to avoid unbounded stall on heavily pruned nodes.
/// Returns the count of mappings written.
fn backfill_number_mappings<Block: BlockT, C, BE>(
	client: &C,
	substrate_backend: &BE,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	from: u64,
	to: u64,
	max_blocks: u64,
) -> Result<u64, String>
where
	C: HeaderBackend<Block>,
	BE: sp_blockchain::Backend<Block>,
{
	let mut written = 0u64;
	for number in from..=to {
		if written >= max_blocks {
			break;
		}
		if frontier_backend
			.mapping()
			.block_hash_by_number(number)?
			.is_some()
		{
			continue;
		}
		let block_number_native = number.saturated_into::<<Block::Header as HeaderT>::Number>();
		let canonical_hash = match client.hash(block_number_native) {
			Ok(Some(hash)) => hash,
			Ok(None) => continue,
			Err(e) => {
				return Err(format!(
					"failed to resolve canonical hash at #{number}: {e:?}"
				))
			}
		};
		if !frontier_backend.mapping().is_synced(&canonical_hash)? {
			continue;
		}
		let header = match substrate_backend.header(canonical_hash) {
			Ok(Some(header)) => header,
			Ok(None) => continue,
			Err(e) => {
				return Err(format!(
					"failed to load canonical header {canonical_hash:?} at #{number}: {e:?}"
				))
			}
		};
		let eth_block_hash = match fp_consensus::find_post_log(header.digest()) {
			Ok(PostLog::Hashes(h)) => Some(h.block_hash),
			Ok(PostLog::Block(block)) => Some(block.header.hash()),
			Ok(PostLog::BlockHash(hash)) => Some(hash),
			Err(_) => match fp_consensus::find_pre_log(header.digest()) {
				Ok(PreLog::Block(block)) => Some(block.header.hash()),
				Err(_) => None,
			},
		};
		if let Some(eth_hash) = eth_block_hash {
			frontier_backend
				.mapping()
				.set_block_hash_by_number(number, eth_hash)?;

			let has_block_mapping = frontier_backend
				.mapping()
				.block_hash(&eth_hash)?
				.map(|hashes| hashes.contains(&canonical_hash))
				.unwrap_or(false);
			if !has_block_mapping {
				let commitment = fc_db::kv::MappingCommitment::<Block> {
					block_hash: canonical_hash,
					ethereum_block_hash: eth_hash,
					ethereum_transaction_hashes: vec![],
				};
				frontier_backend.mapping().write_hashes(
					commitment,
					number,
					fc_db::kv::NumberMappingWrite::Skip,
				)?;
			}

			written += 1;
		}
	}
	if written > 0 {
		log::debug!(
			target: "mapping-sync",
			"Backfilled BLOCK_NUMBER_MAPPING for {written} blocks in #{from}..#{to}",
		);
	}
	Ok(written)
}

pub fn repair_canonical_number_mappings_batch<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	sync_from: <Block::Header as HeaderT>::Number,
	max_blocks: u64,
) -> Result<(), String> {
	if let Some(stats) = canonical_reconciler::reconcile_from_cursor_batch(
		client,
		storage_override,
		frontier_backend,
		sync_from,
		max_blocks,
	)? {
		log::debug!(
			target: "reconcile",
			"batch reconcile scanned {}, updated {}, lag {}",
			stats.scanned,
			stats.updated,
			stats.lag_blocks,
		);
	}

	Ok(())
}

pub fn sync_one_block<Block: BlockT, C, BE>(
	client: &C,
	substrate_backend: &BE,
	storage_override: Arc<dyn StorageOverride<Block>>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	sync_from: <Block::Header as HeaderT>::Number,
	state_pruning_blocks: Option<u64>,
	strategy: SyncStrategy,
	sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
	pubsub_notification_sinks: Arc<
		EthereumBlockNotificationSinks<EthereumBlockNotification<Block>>,
	>,
	best_at_import: &mut HashMap<Block::Hash, BestBlockInfo<Block>>,
) -> Result<bool, String>
where
	C: ProvideRuntimeApi<Block>,
	C::Api: EthereumRuntimeRPCApi<Block>,
	C: HeaderBackend<Block> + StorageProvider<Block, BE>,
	BE: Backend<Block>,
{
	let mut current_syncing_tips = frontier_backend.meta().current_syncing_tips()?;

	if current_syncing_tips.is_empty() {
		let mut leaves = substrate_backend
			.blockchain()
			.leaves()
			.map_err(|e| format!("{e:?}"))?;
		if leaves.is_empty() {
			return Ok(false);
		}
		current_syncing_tips.append(&mut leaves);
	}

	let best_hash = client.info().best_hash;
	if SyncStrategy::Parachain == strategy && !frontier_backend.mapping().is_synced(&best_hash)? {
		// Add best block to current_syncing_tips
		current_syncing_tips.push(best_hash);
	}

	let mut operating_header = None;
	while let Some(checking_tip) = current_syncing_tips.pop() {
		if let Some(checking_header) = fetch_header(
			substrate_backend.blockchain(),
			frontier_backend,
			checking_tip,
			sync_from,
		)? {
			operating_header = Some(checking_header);
			break;
		}
	}
	let operating_header = match operating_header {
		Some(operating_header) => operating_header,
		None => {
			frontier_backend
				.meta()
				.write_current_syncing_tips(current_syncing_tips)?;
			return Ok(false);
		}
	};

	if operating_header.number() == &Zero::zero() {
		sync_genesis_block(client, frontier_backend, &operating_header)?;

		frontier_backend
			.meta()
			.write_current_syncing_tips(current_syncing_tips)?;
	} else {
		if SyncStrategy::Parachain == strategy
			&& operating_header.number() > &client.info().best_number
		{
			return Ok(false);
		}

		// On pruned nodes: live state window is derived from finalized_number (not best),
		// so we skip blocks below (finalized_number - pruning_blocks). That avoids
		// depending on unfinalized chain and matches typical state-pruning semantics.
		// Jump the syncing tip forward to the window floor, retaining any queued tips
		// that are already within the live window (fork/reorg catch-up).
		if let Some(pruning_blocks) = state_pruning_blocks {
			let finalized_number_u64: u64 = client.info().finalized_number.unique_saturated_into();
			let live_window_start_u64 = finalized_number_u64.saturating_sub(pruning_blocks);
			let sync_from_u64: u64 = sync_from.unique_saturated_into();
			let skip_to_u64 = live_window_start_u64.max(sync_from_u64);
			let current_number_u64: u64 = (*operating_header.number()).unique_saturated_into();

			if current_number_u64 < skip_to_u64 {
				let skip_to_number =
					skip_to_u64.saturated_into::<<Block::Header as HeaderT>::Number>();
				match client.hash(skip_to_number) {
					Ok(Some(skip_hash)) => {
						log::warn!(
							target: "mapping-sync",
							"Pruned node: skipping blocks #{}..#{} (outside live state window), \
							jumping tip to #{}",
							current_number_u64,
							skip_to_u64.saturating_sub(1),
							skip_to_u64,
						);
						// Retain any tips still within the live window rather than
						// discarding them — they may be unsynced fork branches that
						// need indexing. Replace only the out-of-window tip with
						// the skip target.
						let mut retained = Vec::with_capacity(current_syncing_tips.len());
						for tip in current_syncing_tips.drain(..) {
							match substrate_backend.blockchain().header(tip) {
								Ok(Some(h)) => {
									let n: u64 = (*h.number()).unique_saturated_into();
									if n >= skip_to_u64 {
										retained.push(tip);
									}
								}
								Ok(None) | Err(_) => {
									retained.push(tip);
								}
							}
						}
						current_syncing_tips = retained;
						current_syncing_tips.push(skip_hash);
						frontier_backend
							.meta()
							.write_current_syncing_tips(current_syncing_tips)?;

						// Backfill BLOCK_NUMBER_MAPPING for already-synced
						// canonical blocks in the live window so
						// latest_block_hash() can find them (handles upgrade
						// from old logic that always used Skip). Capped per
						// call to avoid unbounded stall on heavily pruned nodes.
						let best_number_u64: u64 =
							client.info().best_number.unique_saturated_into();
						backfill_number_mappings(
							client,
							substrate_backend.blockchain(),
							frontier_backend,
							skip_to_u64,
							best_number_u64,
							BACKFILL_ON_SKIP_MAX_BLOCKS,
						)?;

						return Ok(true);
					}
					Ok(None) => {
						// Target block not yet known to the client (e.g. node still
						// syncing headers). Return false to back off and retry later
						// rather than falling through to sync a pruned block.
						current_syncing_tips.push(operating_header.hash());
						frontier_backend
							.meta()
							.write_current_syncing_tips(current_syncing_tips)?;
						return Ok(false);
					}
					Err(e) => {
						// Transient client error. Back off and retry rather than
						// falling through to sync a pruned block.
						log::warn!(
							target: "mapping-sync",
							"Pruned node: failed to resolve skip target #{skip_to_u64}: {e:?}; will retry.",
						);
						current_syncing_tips.push(operating_header.hash());
						frontier_backend
							.meta()
							.write_current_syncing_tips(current_syncing_tips)?;
						return Ok(false);
					}
				}
			}
		}

		sync_block(
			client,
			storage_override.clone(),
			frontier_backend,
			&operating_header,
		)?;

		current_syncing_tips.push(*operating_header.parent_hash());
		frontier_backend
			.meta()
			.write_current_syncing_tips(current_syncing_tips)?;
	}

	// Reconcile the most recent window of blocks.
	canonical_reconciler::reconcile_recent_window(
		client,
		storage_override.as_ref(),
		frontier_backend,
		sync_from,
		PERIODIC_RECONCILE_WINDOW,
	)?;

	// Notify on import and remove closed channels using the unified notification mechanism.
	let hash = operating_header.hash();
	// Use the `is_new_best` status from import time if available.
	// This avoids race conditions where the best hash may have changed
	// between import and sync time (e.g., during rapid reorgs).
	// Fall back to current best hash check for blocks synced during catch-up.
	let best_info = best_at_import.remove(&hash);
	let is_new_best = best_info.is_some() || client.info().best_hash == hash;
	let reorg_info = best_info.and_then(|info| info.reorg_info);

	// Reorg-aware reconcile only when this block was actually new-best at import.
	if is_new_best {
		let reconcile_stats = canonical_reconciler::reconcile_reorg_window(
			client,
			storage_override.as_ref(),
			frontier_backend,
			reorg_info.as_deref(),
			hash,
			sync_from,
		)?;
		log::debug!(
			target: "reconcile",
			"new-best reconcile at {hash:?}: {reconcile_stats:?}",
		);
	}

	emit_block_notification(
		pubsub_notification_sinks.as_ref(),
		sync_oracle.as_ref(),
		BlockNotificationContext {
			hash,
			is_new_best,
			reorg_info,
		},
	);

	Ok(true)
}

pub fn sync_blocks<Block: BlockT, C, BE>(
	client: &C,
	substrate_backend: &BE,
	storage_override: Arc<dyn StorageOverride<Block>>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	limit: usize,
	sync_from: <Block::Header as HeaderT>::Number,
	state_pruning_blocks: Option<u64>,
	strategy: SyncStrategy,
	sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
	pubsub_notification_sinks: Arc<
		EthereumBlockNotificationSinks<EthereumBlockNotification<Block>>,
	>,
	best_at_import: &mut HashMap<Block::Hash, BestBlockInfo<Block>>,
) -> Result<bool, String>
where
	C: ProvideRuntimeApi<Block>,
	C::Api: EthereumRuntimeRPCApi<Block>,
	C: HeaderBackend<Block> + StorageProvider<Block, BE>,
	BE: Backend<Block>,
{
	let mut synced_any = false;

	for _ in 0..limit {
		synced_any = synced_any
			|| sync_one_block(
				client,
				substrate_backend,
				storage_override.clone(),
				frontier_backend,
				sync_from,
				state_pruning_blocks,
				strategy,
				sync_oracle.clone(),
				pubsub_notification_sinks.clone(),
				best_at_import,
			)?;
	}

	// Prune old entries from best_at_import to prevent unbounded growth.
	// Entries for finalized blocks are no longer needed since finalized blocks
	// cannot be reorged and their is_new_best status is irrelevant.
	let finalized_number = client.info().finalized_number;
	best_at_import.retain(|_, info| info.block_number > finalized_number);

	Ok(synced_any)
}

pub fn fetch_header<Block: BlockT, C, BE>(
	substrate_backend: &BE,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	checking_tip: Block::Hash,
	sync_from: <Block::Header as HeaderT>::Number,
) -> Result<Option<Block::Header>, String>
where
	C: HeaderBackend<Block>,
	BE: HeaderBackend<Block>,
{
	if frontier_backend.mapping().is_synced(&checking_tip)? {
		return Ok(None);
	}

	match substrate_backend.header(checking_tip) {
		Ok(Some(checking_header)) if checking_header.number() >= &sync_from => {
			Ok(Some(checking_header))
		}
		Ok(Some(_)) => Ok(None),
		Ok(None) | Err(_) => Err("Header not found".to_string()),
	}
}

#[cfg(test)]
mod tests {
	use std::{collections::HashMap, sync::Arc};

	use ethereum::PartialHeader;
	use ethereum_types::{Address, H256, U256};
	use fc_storage::StorageOverride;
	use fp_rpc::TransactionStatus;
	use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};
	use sc_block_builder::BlockBuilderBuilder;
	use scale_codec::Encode;
	use sp_blockchain::HeaderBackend as _;
	use sp_consensus::BlockOrigin;
	use sp_runtime::{
		generic::Header,
		traits::{BlakeTwo256, Block as BlockT},
		Permill,
	};
	use substrate_test_runtime_client::{
		BlockBuilderExt, ClientBlockImportExt, DefaultTestClientBuilderExt, TestClientBuilder,
		TestClientBuilderExt,
	};
	use tempfile::tempdir;

	use sp_runtime::generic::DigestItem;

	use super::{canonical_reconciler, repair_canonical_number_mappings_batch, sync_one_block};
	use crate::{
		EthereumBlockNotification, EthereumBlockNotificationSinks, ReorgInfo, SyncStrategy,
	};

	fn ethereum_digest_item_for(eth_block: &ethereum::BlockV3) -> DigestItem {
		DigestItem::Consensus(
			fp_consensus::FRONTIER_ENGINE_ID,
			fp_consensus::PostLog::BlockHash(eth_block.header.hash()).encode(),
		)
	}

	type OpaqueBlock = sp_runtime::generic::Block<
		Header<u64, BlakeTwo256>,
		substrate_test_runtime_client::runtime::Extrinsic,
	>;

	struct NoopStorageOverride;

	impl StorageOverride<OpaqueBlock> for NoopStorageOverride {
		fn account_code_at(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
			_address: Address,
		) -> Option<Vec<u8>> {
			None
		}

		fn account_storage_at(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
			_address: Address,
			_index: U256,
		) -> Option<H256> {
			None
		}

		fn current_block(&self, _at: <OpaqueBlock as BlockT>::Hash) -> Option<ethereum::BlockV3> {
			None
		}

		fn current_receipts(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
		) -> Option<Vec<ethereum::ReceiptV4>> {
			None
		}

		fn current_transaction_statuses(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
		) -> Option<Vec<TransactionStatus>> {
			None
		}

		fn elasticity(&self, _at: <OpaqueBlock as BlockT>::Hash) -> Option<Permill> {
			None
		}

		fn is_eip1559(&self, _at: <OpaqueBlock as BlockT>::Hash) -> bool {
			false
		}
	}

	/// Stub SyncOracle for tests that call sync_one_block (not syncing, not offline).
	struct TestSyncOracleNotSyncing;
	impl sp_consensus::SyncOracle for TestSyncOracleNotSyncing {
		fn is_major_syncing(&self) -> bool {
			false
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	fn make_ethereum_block(seed: u64) -> ethereum::BlockV3 {
		make_ethereum_block_inner(seed, vec![])
	}

	fn make_ethereum_block_with_txs(seed: u64, num_txs: u64) -> ethereum::BlockV3 {
		let txs: Vec<ethereum::TransactionV3> = (0..num_txs)
			.map(|i| {
				let sig = ethereum::legacy::TransactionSignature::new(
					27,
					H256::from_low_u64_be(seed.saturating_add(i).saturating_add(1)),
					H256::from_low_u64_be(seed.saturating_add(i).saturating_add(2)),
				)
				.expect("valid signature");
				ethereum::TransactionV3::Legacy(ethereum::LegacyTransaction {
					nonce: U256::from(i),
					gas_price: U256::from(1),
					gas_limit: U256::from(21000),
					action: ethereum::TransactionAction::Call(
						ethereum_types::H160::from_low_u64_be(seed),
					),
					value: U256::zero(),
					input: vec![],
					signature: sig,
				})
			})
			.collect();
		make_ethereum_block_inner(seed, txs)
	}

	fn make_ethereum_block_inner(
		seed: u64,
		transactions: Vec<ethereum::TransactionV3>,
	) -> ethereum::BlockV3 {
		let partial_header = PartialHeader {
			parent_hash: H256::from_low_u64_be(seed),
			beneficiary: ethereum_types::H160::from_low_u64_be(seed),
			state_root: H256::from_low_u64_be(seed.saturating_add(1)),
			receipts_root: H256::from_low_u64_be(seed.saturating_add(2)),
			logs_bloom: ethereum_types::Bloom::default(),
			difficulty: U256::from(seed),
			number: U256::from(seed),
			gas_limit: U256::from(seed.saturating_add(100)),
			gas_used: U256::from(seed.saturating_add(50)),
			timestamp: seed,
			extra_data: Vec::new(),
			mix_hash: H256::from_low_u64_be(seed.saturating_add(3)),
			nonce: ethereum_types::H64::from_low_u64_be(seed),
		};
		ethereum::Block::new(partial_header, transactions, vec![])
	}

	struct SelectiveStorageOverride {
		blocks: HashMap<<OpaqueBlock as BlockT>::Hash, ethereum::BlockV3>,
	}

	impl StorageOverride<OpaqueBlock> for SelectiveStorageOverride {
		fn account_code_at(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
			_address: Address,
		) -> Option<Vec<u8>> {
			None
		}

		fn account_storage_at(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
			_address: Address,
			_index: U256,
		) -> Option<H256> {
			None
		}

		fn current_block(&self, at: <OpaqueBlock as BlockT>::Hash) -> Option<ethereum::BlockV3> {
			self.blocks.get(&at).cloned()
		}

		fn current_receipts(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
		) -> Option<Vec<ethereum::ReceiptV4>> {
			None
		}

		fn current_transaction_statuses(
			&self,
			_at: <OpaqueBlock as BlockT>::Hash,
		) -> Option<Vec<TransactionStatus>> {
			None
		}

		fn elasticity(&self, _at: <OpaqueBlock as BlockT>::Hash) -> Option<Permill> {
			None
		}

		fn is_eip1559(&self, _at: <OpaqueBlock as BlockT>::Hash) -> bool {
			false
		}
	}

	#[test]
	fn non_canonical_new_best_candidate_does_not_advance_pointer() {
		let tmp = tempdir().expect("create temp dir");
		let builder = TestClientBuilder::new();
		let (client, _) = builder
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
				None,
			);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build A1");
		builder
			.push_storage_change(vec![1], None)
			.expect("push storage change for A1");
		let a1 = builder.build().expect("build A1 block").block;
		let a1_hash = a1.header.hash();
		futures::executor::block_on(client.import(BlockOrigin::Own, a1)).expect("import A1");

		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(a1_hash)
			.fetch_parent_block_number(client.as_ref())
			.expect("fetch A1 number")
			.build()
			.expect("build A2");
		builder
			.push_storage_change(vec![2], None)
			.expect("push storage change for A2");
		let a2 = builder.build().expect("build A2 block").block;
		futures::executor::block_on(client.import(BlockOrigin::Own, a2)).expect("import A2");

		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build B1");
		builder
			.push_storage_change(vec![3], None)
			.expect("push storage change for B1");
		let b1 = builder.build().expect("build B1 block").block;
		let b1_hash = b1.header.hash();
		futures::executor::block_on(client.import(BlockOrigin::Own, b1)).expect("import B1");

		assert_eq!(client.hash(1).expect("hash query"), Some(a1_hash));
		assert_ne!(client.hash(1).expect("hash query"), Some(b1_hash));

		frontier_backend
			.mapping()
			.set_latest_canonical_indexed_block(1)
			.expect("seed pointer");

		let repaired = canonical_reconciler::reconcile_reorg_window(
			client.as_ref(),
			&NoopStorageOverride,
			&frontier_backend,
			None,
			b1_hash,
			1,
		)
		.expect("repair pass");

		assert_eq!(
			repaired,
			Some(canonical_reconciler::ReconcileStats {
				scanned: 1,
				updated: 0,
				first_unresolved: Some(1),
				highest_reconciled: None,
				next_cursor: 1,
				lag_blocks: 1,
				window: canonical_reconciler::ReconcileWindow { start: 1, end: 1 },
			})
		);
		assert_eq!(
			frontier_backend
				.mapping()
				.latest_canonical_indexed_block_number()
				.expect("pointer read"),
			Some(1)
		);
	}

	#[test]
	fn canonical_number_repair_retries_unresolved_blocks_without_skipping_cursor() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block 1");
		builder
			.push_storage_change(vec![1], None)
			.expect("push storage change for block 1");
		let block_1 = builder.build().expect("build block 1").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block_1))
			.expect("import block 1");

		let best_after_1 = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(best_after_1.best_hash)
			.with_parent_block_number(best_after_1.best_number)
			.build()
			.expect("build block 2");
		builder
			.push_storage_change(vec![2], None)
			.expect("push storage change for block 2");
		let block_2 = builder.build().expect("build block 2").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block_2))
			.expect("import block 2");

		let canonical_hash_1 = client
			.hash(1)
			.expect("query canonical hash for #1")
			.expect("canonical hash for #1");
		let canonical_hash_2 = client
			.hash(2)
			.expect("query canonical hash for #2")
			.expect("canonical hash for #2");
		let eth_block_2 = make_ethereum_block(2);
		let eth_hash_2 = eth_block_2.header.hash();
		let storage_override = SelectiveStorageOverride {
			blocks: HashMap::from([(canonical_hash_2, eth_block_2)]),
		};

		repair_canonical_number_mappings_batch(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			1,
			2,
		)
		.expect("run repair batch");

		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1),
			Ok(None),
			"block #1 remains unresolved"
		);
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(2),
			Ok(Some(eth_hash_2)),
			"block #2 can still be repaired in the same pass"
		);
		assert_eq!(
			frontier_backend.mapping().canonical_number_repair_cursor(),
			Ok(Some(1)),
			"cursor must stay at first unresolved block for retry"
		);

		assert!(storage_override.current_block(canonical_hash_1).is_none());
	}

	#[test]
	fn reconcile_reorg_window_does_not_write_below_sync_from() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block 1");
		builder
			.push_storage_change(vec![1], None)
			.expect("push storage change for block 1");
		let block_1 = builder.build().expect("build block 1").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block_1))
			.expect("import block 1");

		let best_after_1 = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(best_after_1.best_hash)
			.with_parent_block_number(best_after_1.best_number)
			.build()
			.expect("build block 2");
		builder
			.push_storage_change(vec![2], None)
			.expect("push storage change for block 2");
		let block_2 = builder.build().expect("build block 2").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block_2))
			.expect("import block 2");

		let best_after_2 = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(best_after_2.best_hash)
			.with_parent_block_number(best_after_2.best_number)
			.build()
			.expect("build block 3");
		builder
			.push_storage_change(vec![3], None)
			.expect("push storage change for block 3");
		let block_3 = builder.build().expect("build block 3").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block_3))
			.expect("import block 3");

		let canonical_hash_1 = client
			.hash(1)
			.expect("query canonical hash for #1")
			.expect("canonical hash for #1");
		let canonical_hash_2 = client
			.hash(2)
			.expect("query canonical hash for #2")
			.expect("canonical hash for #2");
		let canonical_hash_3 = client
			.hash(3)
			.expect("query canonical hash for #3")
			.expect("canonical hash for #3");

		let eth_block_2 = make_ethereum_block(2);
		let eth_block_3 = make_ethereum_block(3);
		let eth_hash_3 = eth_block_3.header.hash();
		let storage_override = SelectiveStorageOverride {
			blocks: HashMap::from([
				(canonical_hash_2, eth_block_2),
				(canonical_hash_3, eth_block_3),
			]),
		};

		frontier_backend
			.mapping()
			.set_block_hash_by_number(2, H256::repeat_byte(0x22))
			.expect("seed stale #2");
		frontier_backend
			.mapping()
			.set_block_hash_by_number(3, H256::repeat_byte(0x33))
			.expect("seed stale #3");

		let reorg_info = ReorgInfo::<OpaqueBlock> {
			common_ancestor: canonical_hash_1,
			retracted: vec![],
			enacted: vec![canonical_hash_2],
			new_best: canonical_hash_3,
		};
		let stats = canonical_reconciler::reconcile_reorg_window(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			Some(&reorg_info),
			canonical_hash_3,
			3,
		)
		.expect("reconcile reorg window")
		.expect("stats");

		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(2),
			Ok(Some(H256::repeat_byte(0x22))),
			"mapping below sync_from must stay unchanged",
		);
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(3),
			Ok(Some(eth_hash_3)),
			"mapping at sync_from must be reconciled",
		);
		assert_eq!(stats.scanned, 1);
		assert_eq!(stats.updated, 1);
		assert_eq!(
			stats.window,
			canonical_reconciler::ReconcileWindow { start: 3, end: 3 },
		);
	}

	#[test]
	fn canonical_reconcile_is_idempotent_and_pointer_monotonic() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block");
		builder
			.push_storage_change(vec![1], None)
			.expect("push storage change");
		let block = builder.build().expect("build block").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block))
			.expect("import block");

		let canonical_hash = client
			.hash(1)
			.expect("query canonical hash")
			.expect("canonical hash");
		let canonical_eth_block = make_ethereum_block(1);
		let canonical_eth_hash = canonical_eth_block.header.hash();
		let storage_override = SelectiveStorageOverride {
			blocks: HashMap::from([(canonical_hash, canonical_eth_block)]),
		};

		frontier_backend
			.mapping()
			.set_block_hash_by_number(1, H256::repeat_byte(0x55))
			.expect("seed stale mapping");

		let first = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			1,
			1,
		)
		.expect("first reconcile")
		.expect("stats");
		assert_eq!(first.updated, 1);
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1),
			Ok(Some(canonical_eth_hash))
		);
		let pointer_after_first = frontier_backend
			.mapping()
			.latest_canonical_indexed_block_number()
			.expect("read pointer after first")
			.expect("pointer after first");

		let second = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			1,
			1,
		)
		.expect("second reconcile")
		.expect("stats");
		assert_eq!(second.updated, 0);
		let pointer_after_second = frontier_backend
			.mapping()
			.latest_canonical_indexed_block_number()
			.expect("read pointer after second")
			.expect("pointer after second");
		assert!(
			pointer_after_second >= pointer_after_first,
			"latest canonical pointer must be monotonic"
		);
	}

	#[test]
	fn canonical_reconcile_batch_prioritizes_recent_finalized_blocks() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block 1");
		builder
			.push_storage_change(vec![1], None)
			.expect("push storage change for block 1");
		let block_1 = builder.build().expect("build block 1").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block_1))
			.expect("import block 1");

		let best_after_1 = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(best_after_1.best_hash)
			.with_parent_block_number(best_after_1.best_number)
			.build()
			.expect("build block 2");
		builder
			.push_storage_change(vec![2], None)
			.expect("push storage change for block 2");
		let block_2 = builder.build().expect("build block 2").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block_2))
			.expect("import block 2");

		let canonical_hash_1 = client
			.hash(1)
			.expect("query canonical hash for #1")
			.expect("canonical hash for #1");
		let canonical_hash_2 = client
			.hash(2)
			.expect("query canonical hash for #2")
			.expect("canonical hash for #2");
		let eth_block_1 = make_ethereum_block(1);
		let eth_hash_1 = eth_block_1.header.hash();
		let eth_block_2 = make_ethereum_block(2);
		let eth_hash_2 = eth_block_2.header.hash();
		let storage_override = SelectiveStorageOverride {
			blocks: HashMap::from([
				(canonical_hash_1, eth_block_1),
				(canonical_hash_2, eth_block_2),
			]),
		};

		frontier_backend
			.mapping()
			.set_block_hash_by_number(1, H256::repeat_byte(0x11))
			.expect("seed stale #1");
		frontier_backend
			.mapping()
			.set_block_hash_by_number(2, H256::repeat_byte(0x22))
			.expect("seed stale #2");

		let first = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			0,
			1,
		)
		.expect("first batch")
		.expect("first stats");
		assert_eq!(first.scanned, 1);
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(2),
			Ok(Some(eth_hash_2)),
			"latest finalized block must be repaired first"
		);
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1),
			Ok(Some(H256::repeat_byte(0x11))),
			"older block should still be stale after first small batch"
		);

		let second = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			0,
			1,
		)
		.expect("second batch")
		.expect("second stats");
		assert_eq!(second.scanned, 1);
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1),
			Ok(Some(eth_hash_1)),
			"second batch should continue backward"
		);
	}

	/// After a pruning skip, tips that are within the live window (>= skip_to) must be
	/// retained so fork/reorg catch-up can continue. Window is derived from finalized_number.
	#[test]
	fn pruning_skip_retains_in_window_tips() {
		let tmp = tempdir().expect("create temp dir");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		// Build chain 0..=10 and finalize so finalized_number = 10.
		let mut chain_info = client.chain_info();
		for _ in 1..=10 {
			let mut block_builder = BlockBuilderBuilder::new(client.as_ref())
				.on_parent_block(chain_info.best_hash)
				.with_parent_block_number(chain_info.best_number)
				.build()
				.expect("build block");
			block_builder
				.push_storage_change(vec![1], None)
				.expect("push storage change");
			let block = block_builder.build().expect("build block").block;
			futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block))
				.expect("import as final");
			chain_info = client.chain_info();
		}
		assert!(
			chain_info.finalized_number >= 10,
			"finalized number for pruning test"
		);

		let hash_1 = client.hash(1).expect("hash").expect("block 1 exists");
		let hash_2 = client.hash(2).expect("hash").expect("block 2 exists");
		let hash_5 = client.hash(5).expect("hash").expect("block 5 exists");

		// Tips: one below window (1), one in window (5). With state_pruning_blocks=8,
		// live_window_start = 10 - 8 = 2, skip_to = 2. Order so the below-window tip is popped
		// first (sync_one_block pops from the end): [in_window, below_window].
		frontier_backend
			.meta()
			.write_current_syncing_tips(vec![hash_5, hash_1])
			.expect("write tips");

		let storage_override: Arc<dyn fc_storage::StorageOverride<OpaqueBlock>> =
			Arc::new(NoopStorageOverride);
		let sync_oracle: Arc<dyn sp_consensus::SyncOracle + Send + Sync> =
			Arc::new(TestSyncOracleNotSyncing);
		let pubsub_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<OpaqueBlock>>,
		> = Arc::new(Default::default());
		let mut best_at_import = HashMap::new();

		let did_sync = sync_one_block(
			client.as_ref(),
			&backend,
			storage_override,
			&frontier_backend,
			0,
			Some(8),
			SyncStrategy::Normal,
			sync_oracle,
			pubsub_sinks,
			&mut best_at_import,
		)
		.expect("sync_one_block");
		assert!(did_sync, "skip path should run and return true");

		let tips = frontier_backend
			.meta()
			.current_syncing_tips()
			.expect("read tips");
		assert!(
			tips.contains(&hash_5),
			"in-window tip (block 5) must be retained after skip; tips={tips:?}",
		);
		assert!(
			tips.contains(&hash_2),
			"skip target (block 2) must be in tips after skip; tips={tips:?}",
		);
	}

	/// Reconciler None branch: when a block has SYNCED_MAPPING + BLOCK_NUMBER_MAPPING
	/// but no BLOCK_MAPPING (the old write_none + backfill path), the reconciler must
	/// verify the eth hash from the header digest and repair BLOCK_MAPPING so
	/// indexed_canonical_hash_at() can resolve the block.
	#[test]
	fn reconciler_repairs_missing_block_mapping_on_pruned_blocks() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let eth_block = make_ethereum_block(1);
		let eth_hash = eth_block.header.hash();

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block 1");
		builder
			.push_deposit_log_digest_item(ethereum_digest_item_for(&eth_block))
			.expect("push ethereum digest");
		let block = builder.build().expect("build block").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block))
			.expect("import block");

		let canonical_hash = client
			.hash(1)
			.expect("query canonical hash")
			.expect("canonical hash");

		// Simulate old write_none path: only SYNCED_MAPPING is set.
		frontier_backend
			.mapping()
			.write_none(canonical_hash)
			.expect("write_none");

		// Simulate backfill: BLOCK_NUMBER_MAPPING is set, but BLOCK_MAPPING is NOT.
		frontier_backend
			.mapping()
			.set_block_hash_by_number(1, eth_hash)
			.expect("set block hash by number");

		// Sanity: BLOCK_MAPPING must be absent.
		assert_eq!(
			frontier_backend.mapping().block_hash(&eth_hash),
			Ok(None),
			"BLOCK_MAPPING must be absent before reconciler runs"
		);

		// Run reconciler with NoopStorageOverride (state unavailable → hits None branch).
		let stats = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&NoopStorageOverride,
			&frontier_backend,
			1,
			1,
		)
		.expect("reconcile")
		.expect("stats");

		// BLOCK_MAPPING must now contain the canonical hash.
		let block_mapping = frontier_backend
			.mapping()
			.block_hash(&eth_hash)
			.expect("read BLOCK_MAPPING");
		assert!(
			block_mapping
				.as_ref()
				.is_some_and(|hashes| hashes.contains(&canonical_hash)),
			"reconciler must repair BLOCK_MAPPING; got {block_mapping:?}"
		);
		assert_eq!(stats.updated, 1, "reconciler must report 1 update");
	}

	/// Reconciler None branch: when BLOCK_NUMBER_MAPPING holds a stale eth hash
	/// (e.g. after a reorg), the reconciler must re-derive the correct eth hash
	/// from the header digest, correct BLOCK_NUMBER_MAPPING, and write BLOCK_MAPPING
	/// with the verified hash — not the stale one.
	#[test]
	fn reconciler_corrects_stale_block_number_mapping_after_reorg() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let correct_eth_block = make_ethereum_block(1);
		let correct_eth_hash = correct_eth_block.header.hash();
		let stale_eth_hash = make_ethereum_block(99).header.hash();
		assert_ne!(correct_eth_hash, stale_eth_hash);

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block 1");
		builder
			.push_deposit_log_digest_item(ethereum_digest_item_for(&correct_eth_block))
			.expect("push ethereum digest");
		let block = builder.build().expect("build block").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block))
			.expect("import block");

		let canonical_hash = client
			.hash(1)
			.expect("query canonical hash")
			.expect("canonical hash");

		// Simulate a stale BLOCK_NUMBER_MAPPING from a pre-reorg fork.
		frontier_backend
			.mapping()
			.set_block_hash_by_number(1, stale_eth_hash)
			.expect("set stale block hash by number");

		// Run reconciler with NoopStorageOverride (state unavailable → hits None branch).
		let stats = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&NoopStorageOverride,
			&frontier_backend,
			1,
			1,
		)
		.expect("reconcile")
		.expect("stats");

		// BLOCK_NUMBER_MAPPING must now hold the correct (digest-derived) eth hash.
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1),
			Ok(Some(correct_eth_hash)),
			"reconciler must correct stale BLOCK_NUMBER_MAPPING to digest-derived hash"
		);

		// BLOCK_MAPPING must map the correct eth hash to the canonical substrate hash.
		let block_mapping = frontier_backend
			.mapping()
			.block_hash(&correct_eth_hash)
			.expect("read BLOCK_MAPPING for correct hash");
		assert!(
			block_mapping
				.as_ref()
				.is_some_and(|hashes| hashes.contains(&canonical_hash)),
			"BLOCK_MAPPING must use the verified eth hash; got {block_mapping:?}"
		);

		// The stale eth hash must NOT have a BLOCK_MAPPING pointing to the canonical hash.
		let stale_mapping = frontier_backend
			.mapping()
			.block_hash(&stale_eth_hash)
			.expect("read BLOCK_MAPPING for stale hash");
		assert!(
			!stale_mapping
				.as_ref()
				.is_some_and(|hashes| hashes.contains(&canonical_hash)),
			"stale eth hash must not be mapped to canonical substrate hash; got {stale_mapping:?}"
		);

		assert!(stats.updated >= 1, "reconciler must report updates");
	}

	/// When the reconciler encounters an unsynced block and state is available,
	/// it must write BLOCK_MAPPING with the canonical hash so the block becomes
	/// resolvable by indexed_canonical_hash_at / latest_block_hash.
	#[test]
	fn reconciler_writes_block_mapping_for_unsynced_blocks() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block 1");
		builder
			.push_storage_change(vec![1], None)
			.expect("push storage change");
		let block = builder.build().expect("build block").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block))
			.expect("import block");

		let canonical_hash = client
			.hash(1)
			.expect("query canonical hash")
			.expect("canonical hash");
		let eth_block = make_ethereum_block(1);
		let eth_hash = eth_block.header.hash();
		let storage_override = SelectiveStorageOverride {
			blocks: HashMap::from([(canonical_hash, eth_block)]),
		};

		// Block is completely unsynced: no SYNCED, no BLOCK_MAPPING, no BLOCK_NUMBER_MAPPING.
		assert_eq!(
			frontier_backend.mapping().is_synced(&canonical_hash),
			Ok(false),
		);
		assert_eq!(frontier_backend.mapping().block_hash(&eth_hash), Ok(None));
		assert_eq!(frontier_backend.mapping().block_hash_by_number(1), Ok(None));

		// Run reconciler with state available.
		let stats = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			1,
			1,
		)
		.expect("reconcile")
		.expect("stats");

		// BLOCK_MAPPING must now contain the canonical hash.
		let block_mapping = frontier_backend
			.mapping()
			.block_hash(&eth_hash)
			.expect("read BLOCK_MAPPING");
		assert!(
			block_mapping
				.as_ref()
				.is_some_and(|hashes| hashes.contains(&canonical_hash)),
			"reconciler must write BLOCK_MAPPING for unsynced blocks; got {block_mapping:?}"
		);

		// BLOCK_NUMBER_MAPPING must be set.
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1),
			Ok(Some(eth_hash)),
			"reconciler must write BLOCK_NUMBER_MAPPING"
		);

		// is_synced must now be true (write_hashes sets SYNCED_MAPPING).
		assert_eq!(
			frontier_backend.mapping().is_synced(&canonical_hash),
			Ok(true),
			"block must be marked as synced after reconciliation"
		);

		assert_eq!(stats.updated, 1);
		assert!(stats.highest_reconciled.is_some());
	}

	/// When a block was synced with empty tx hashes (pruned-state path), the reconciler
	/// must repair TRANSACTION_MAPPING when state becomes available. This ensures
	/// eth_getTransactionByHash works for blocks that were initially synced without state.
	#[test]
	fn reconciler_repairs_missing_transaction_mapping_on_pruned_blocks() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let frontier_backend = fc_db::kv::Backend::<OpaqueBlock, _>::new(
			client.clone(),
			&fc_db::kv::DatabaseSettings {
				#[cfg(feature = "rocksdb")]
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tmp.path().to_path_buf(),
					cache_size: 0,
				},
				#[cfg(not(feature = "rocksdb"))]
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tmp.path().to_path_buf(),
				},
			},
		)
		.expect("frontier backend");

		let eth_block = make_ethereum_block_with_txs(1, 2);
		let eth_hash = eth_block.header.hash();
		let tx_hashes: Vec<H256> = eth_block.transactions.iter().map(|tx| tx.hash()).collect();
		assert_eq!(tx_hashes.len(), 2);

		let chain = client.chain_info();
		let mut builder = BlockBuilderBuilder::new(client.as_ref())
			.on_parent_block(chain.best_hash)
			.with_parent_block_number(chain.best_number)
			.build()
			.expect("build block 1");
		builder
			.push_storage_change(vec![1], None)
			.expect("push storage change");
		let block = builder.build().expect("build block").block;
		futures::executor::block_on(client.import_as_final(BlockOrigin::Own, block))
			.expect("import block");

		let canonical_hash = client
			.hash(1)
			.expect("query canonical hash")
			.expect("canonical hash");

		// Simulate pruned-state sync: write_hashes with empty tx list.
		// This writes BLOCK_MAPPING, BLOCK_NUMBER_MAPPING, SYNCED_MAPPING but
		// no TRANSACTION_MAPPING entries.
		let minimal_commitment = fc_db::kv::MappingCommitment::<OpaqueBlock> {
			block_hash: canonical_hash,
			ethereum_block_hash: eth_hash,
			ethereum_transaction_hashes: vec![],
		};
		frontier_backend
			.mapping()
			.write_hashes(minimal_commitment, 1, fc_db::kv::NumberMappingWrite::Write)
			.expect("write minimal commitment");

		// Sanity: BLOCK_MAPPING exists, TRANSACTION_MAPPING is empty.
		assert!(
			frontier_backend
				.mapping()
				.block_hash(&eth_hash)
				.expect("read BLOCK_MAPPING")
				.is_some_and(|hashes| hashes.contains(&canonical_hash)),
			"BLOCK_MAPPING must exist"
		);
		assert!(
			frontier_backend
				.mapping()
				.transaction_metadata(&tx_hashes[0])
				.expect("read tx metadata")
				.is_empty(),
			"TRANSACTION_MAPPING must be empty before repair"
		);

		// Run reconciler with state now available (SelectiveStorageOverride
		// returns the ethereum block with transactions).
		let storage_override = SelectiveStorageOverride {
			blocks: HashMap::from([(canonical_hash, eth_block)]),
		};
		let stats = canonical_reconciler::reconcile_from_cursor_batch(
			client.as_ref(),
			&storage_override,
			&frontier_backend,
			1,
			1,
		)
		.expect("reconcile")
		.expect("stats");

		// TRANSACTION_MAPPING must now be populated for both transactions.
		for (i, tx_hash) in tx_hashes.iter().enumerate() {
			let metadata = frontier_backend
				.mapping()
				.transaction_metadata(tx_hash)
				.expect("read tx metadata");
			assert!(
				metadata
					.iter()
					.any(|m| m.substrate_block_hash == canonical_hash
						&& m.ethereum_index == i as u32),
				"tx {i} ({tx_hash:?}) must have TRANSACTION_MAPPING for canonical block; got {metadata:?}"
			);
		}

		assert_eq!(
			stats.updated, 1,
			"reconciler must report 1 update for tx repair"
		);
	}
}
