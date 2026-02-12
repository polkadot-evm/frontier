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

mod worker;

pub use worker::MappingSyncWorker;

use std::{collections::HashMap, sync::Arc};

// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::{Backend as _, HeaderBackend};
use sp_consensus::SyncOracle;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto, Zero};
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

pub fn sync_block<Block: BlockT, C: HeaderBackend<Block>>(
	storage_override: Arc<dyn StorageOverride<Block>>,
	backend: &fc_db::kv::Backend<Block, C>,
	header: &Block::Header,
	write_number_mapping: bool,
) -> Result<(), String> {
	let substrate_block_hash = header.hash();
	let block_number: u64 = (*header.number()).unique_saturated_into();
	let number_mapping_write = if write_number_mapping {
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
							None => backend.mapping().write_none(substrate_block_hash),
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

fn repair_canonical_number_mapping_for_hash<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	hash: Block::Hash,
) -> Result<Option<u64>, String> {
	let Some(header) = client.header(hash).map_err(|e| format!("{e:?}"))? else {
		return Ok(None);
	};
	let block_number: u64 = (*header.number()).unique_saturated_into();
	let is_canonical_now = client
		.hash(block_number.unique_saturated_into())
		.map_err(|e| format!("{e:?}"))?
		== Some(hash);
	if !is_canonical_now {
		return Ok(None);
	}
	let Some(ethereum_block) = storage_override.current_block(hash) else {
		return Ok(None);
	};
	frontier_backend
		.mapping()
		.set_block_hash_by_number(block_number, ethereum_block.header.hash())?;
	Ok(Some(block_number))
}

pub fn repair_canonical_number_mappings_batch<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	sync_from: <Block::Header as HeaderT>::Number,
	max_blocks: u64,
) -> Result<(), String> {
	if max_blocks == 0 {
		return Ok(());
	}

	let best_number: u64 = client.info().best_number.unique_saturated_into();
	let sync_from_number: u64 =
		UniqueSaturatedInto::<u64>::unique_saturated_into(sync_from).min(best_number);
	let cursor = frontier_backend
		.mapping()
		.canonical_number_repair_cursor()?
		.unwrap_or(sync_from_number)
		.max(sync_from_number)
		.min(best_number);

	let end = cursor
		.saturating_add(max_blocks.saturating_sub(1))
		.min(best_number);

	let mut repaired = 0u64;
	let mut first_unresolved = None;
	for number in cursor..=end {
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
		let should_repair =
			frontier_backend.mapping().block_hash_by_number(number)? != Some(canonical_eth_hash);
		if should_repair {
			frontier_backend
				.mapping()
				.set_block_hash_by_number(number, canonical_eth_hash)?;
			repaired = repaired.saturating_add(1);
		}
	}

	let next_cursor = if let Some(unresolved) = first_unresolved {
		unresolved
	} else if end >= best_number {
		best_number
	} else {
		end.saturating_add(1)
	};
	frontier_backend
		.mapping()
		.set_canonical_number_repair_cursor(next_cursor)?;

	log::debug!(
		target: "mapping-sync",
		"canonical number repair scanned #{cursor}..#{end}, repaired {repaired}, first unresolved {first_unresolved:?}, next cursor #{next_cursor}",
	);

	Ok(())
}

fn advance_latest_canonical_indexed_block<Block: BlockT, C: HeaderBackend<Block>>(
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	block_number: u64,
) -> Result<(), String> {
	let latest_indexed = frontier_backend
		.mapping()
		.latest_canonical_indexed_block_number()?;
	if latest_indexed.map_or(true, |current| block_number > current) {
		frontier_backend
			.mapping()
			.set_latest_canonical_indexed_block(block_number)?;
	}
	Ok(())
}

fn repair_new_best_number_mappings<Block: BlockT, C: HeaderBackend<Block>>(
	client: &C,
	storage_override: &dyn StorageOverride<Block>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	hash: Block::Hash,
	reorg_info: Option<&crate::ReorgInfo<Block>>,
) -> Result<u64, String> {
	// `is_new_best` can come from import-time state and may be stale by sync time.
	// Number mapping repairs are canonical-gated in `repair_canonical_number_mapping_for_hash`.
	let mut reorg_remapped = 0u64;
	if let Some(repaired_number) =
		repair_canonical_number_mapping_for_hash(client, storage_override, frontier_backend, hash)?
	{
		advance_latest_canonical_indexed_block(frontier_backend, repaired_number)?;
		reorg_remapped = reorg_remapped.saturating_add(1);
	} else {
		log::debug!(
			target: "mapping-sync",
			"Skipping canonical pointer update for non-canonical new-best candidate {hash:?}",
		);
	}
	if let Some(info) = reorg_info {
		for enacted_hash in &info.enacted {
			if let Some(repaired_number) = repair_canonical_number_mapping_for_hash(
				client,
				storage_override,
				frontier_backend,
				*enacted_hash,
			)? {
				advance_latest_canonical_indexed_block(frontier_backend, repaired_number)?;
				reorg_remapped = reorg_remapped.saturating_add(1);
			}
		}
	}

	Ok(reorg_remapped)
}

pub fn sync_one_block<Block: BlockT, C, BE>(
	client: &C,
	substrate_backend: &BE,
	storage_override: Arc<dyn StorageOverride<Block>>,
	frontier_backend: &fc_db::kv::Backend<Block, C>,
	sync_from: <Block::Header as HeaderT>::Number,
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
		let block_number: u64 = (*operating_header.number()).unique_saturated_into();
		let is_canonical_now = client
			.hash(block_number.unique_saturated_into())
			.map_err(|e| format!("{e:?}"))?
			== Some(operating_header.hash());
		if !is_canonical_now {
			log::debug!(
				target: "mapping-sync",
				"Skipping block-number mapping write for non-canonical block #{} ({:?})",
				operating_header.number(),
				operating_header.hash(),
			);
		}
		sync_block(
			storage_override.clone(),
			frontier_backend,
			&operating_header,
			is_canonical_now,
		)?;
		if is_canonical_now {
			advance_latest_canonical_indexed_block(frontier_backend, block_number)?;
		}

		current_syncing_tips.push(*operating_header.parent_hash());
		frontier_backend
			.meta()
			.write_current_syncing_tips(current_syncing_tips)?;
	}
	// Notify on import and remove closed channels using the unified notification mechanism.
	let hash = operating_header.hash();
	// Use the `is_new_best` status from import time if available.
	// This avoids race conditions where the best hash may have changed
	// between import and sync time (e.g., during rapid reorgs).
	// Fall back to current best hash check for blocks synced during catch-up.
	let best_info = best_at_import.remove(&hash);
	let is_new_best = best_info.is_some() || client.info().best_hash == hash;
	let reorg_info = best_info.and_then(|info| info.reorg_info);

	if is_new_best {
		let reorg_remapped = repair_new_best_number_mappings(
			client,
			storage_override.as_ref(),
			frontier_backend,
			hash,
			reorg_info.as_deref(),
		)?;
		log::debug!(
			target: "mapping-sync",
			"Reorg canonical remap touched {reorg_remapped} blocks at new best {hash:?}",
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
	use sc_block_builder::BlockBuilderBuilder;
	use sp_blockchain::HeaderBackend as _;
	use sp_consensus::BlockOrigin;
	use sp_runtime::{
		generic::Header,
		traits::{BlakeTwo256, Block as BlockT},
		Permill,
	};
	use substrate_test_runtime_client::{
		BlockBuilderExt, ClientBlockImportExt, DefaultTestClientBuilderExt, TestClientBuilder,
	};
	use tempfile::tempdir;

	use super::{repair_canonical_number_mappings_batch, repair_new_best_number_mappings};

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

	fn make_ethereum_block(seed: u64) -> ethereum::BlockV3 {
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
		ethereum::Block::new(partial_header, vec![], vec![])
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

		let repaired = repair_new_best_number_mappings(
			client.as_ref(),
			&NoopStorageOverride,
			&frontier_backend,
			b1_hash,
			None,
		)
		.expect("repair pass");

		assert_eq!(repaired, 0);
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
		futures::executor::block_on(client.import(BlockOrigin::Own, block_1))
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
		futures::executor::block_on(client.import(BlockOrigin::Own, block_2))
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
}
