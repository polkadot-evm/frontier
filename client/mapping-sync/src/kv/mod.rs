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

#![allow(clippy::too_many_arguments)]

mod worker;

pub use worker::MappingSyncWorker;

use std::sync::Arc;

// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::{Backend as _, HeaderBackend};
use sp_consensus::SyncOracle;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Zero};
// Frontier
use fc_storage::OverrideHandle;
use fp_consensus::{FindLogError, Hashes, Log, PostLog, PreLog};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{EthereumBlockNotification, EthereumBlockNotificationSinks, SyncStrategy};

pub fn sync_block<Block: BlockT, C, BE>(
	client: &C,
	overrides: Arc<OverrideHandle<Block>>,
	backend: &fc_db::kv::Backend<Block>,
	header: &Block::Header,
) -> Result<(), String>
where
	C: HeaderBackend<Block> + StorageProvider<Block, BE>,
	BE: Backend<Block>,
{
	let substrate_block_hash = header.hash();
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
					backend.mapping().write_hashes(mapping_commitment)
				}
				Log::Post(post_log) => match post_log {
					PostLog::Hashes(hashes) => {
						let mapping_commitment = gen_from_hashes(hashes);
						backend.mapping().write_hashes(mapping_commitment)
					}
					PostLog::Block(block) => {
						let mapping_commitment = gen_from_block(block);
						backend.mapping().write_hashes(mapping_commitment)
					}
					PostLog::BlockHash(expect_eth_block_hash) => {
						let schema =
							fc_storage::onchain_storage_schema(client, substrate_block_hash);
						let ethereum_block = overrides
							.schemas
							.get(&schema)
							.unwrap_or(&overrides.fallback)
							.current_block(substrate_block_hash);
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
									backend.mapping().write_hashes(mapping_commitment)
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
	backend: &fc_db::kv::Backend<Block>,
	header: &Block::Header,
) -> Result<(), String>
where
	C: ProvideRuntimeApi<Block>,
	C::Api: EthereumRuntimeRPCApi<Block>,
{
	let substrate_block_hash = header.hash();

	if let Some(api_version) = client
		.runtime_api()
		.api_version::<dyn EthereumRuntimeRPCApi<Block>>(substrate_block_hash)
		.map_err(|e| format!("{:?}", e))?
	{
		let block = if api_version > 1 {
			client
				.runtime_api()
				.current_block(substrate_block_hash)
				.map_err(|e| format!("{:?}", e))?
		} else {
			#[allow(deprecated)]
			let legacy_block = client
				.runtime_api()
				.current_block_before_version_2(substrate_block_hash)
				.map_err(|e| format!("{:?}", e))?;
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
		backend.mapping().write_hashes(mapping_commitment)?;
	} else {
		backend.mapping().write_none(substrate_block_hash)?;
	};

	Ok(())
}

pub fn sync_one_block<Block: BlockT, C, BE>(
	client: &C,
	substrate_backend: &BE,
	overrides: Arc<OverrideHandle<Block>>,
	frontier_backend: &fc_db::kv::Backend<Block>,
	sync_from: <Block::Header as HeaderT>::Number,
	strategy: SyncStrategy,
	sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
	pubsub_notification_sinks: Arc<
		EthereumBlockNotificationSinks<EthereumBlockNotification<Block>>,
	>,
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
			.map_err(|e| format!("{:?}", e))?;
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
		sync_block(client, overrides, frontier_backend, &operating_header)?;

		current_syncing_tips.push(*operating_header.parent_hash());
		frontier_backend
			.meta()
			.write_current_syncing_tips(current_syncing_tips)?;
	}
	// Notify on import and remove closed channels.
	// Only notify when the node is node in major syncing.
	let sinks = &mut pubsub_notification_sinks.lock();
	sinks.retain(|sink| {
		if !sync_oracle.is_major_syncing() {
			let hash = operating_header.hash();
			let is_new_best = client.info().best_hash == hash;
			sink.unbounded_send(EthereumBlockNotification { is_new_best, hash })
				.is_ok()
		} else {
			// Remove from the pool if in major syncing.
			false
		}
	});
	Ok(true)
}

pub fn sync_blocks<Block: BlockT, C, BE>(
	client: &C,
	substrate_backend: &BE,
	overrides: Arc<OverrideHandle<Block>>,
	frontier_backend: &fc_db::kv::Backend<Block>,
	limit: usize,
	sync_from: <Block::Header as HeaderT>::Number,
	strategy: SyncStrategy,
	sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
	pubsub_notification_sinks: Arc<
		EthereumBlockNotificationSinks<EthereumBlockNotification<Block>>,
	>,
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
				overrides.clone(),
				frontier_backend,
				sync_from,
				strategy,
				sync_oracle.clone(),
				pubsub_notification_sinks.clone(),
			)?;
	}

	Ok(synced_any)
}

pub fn fetch_header<Block: BlockT, BE>(
	substrate_backend: &BE,
	frontier_backend: &fc_db::kv::Backend<Block>,
	checking_tip: Block::Hash,
	sync_from: <Block::Header as HeaderT>::Number,
) -> Result<Option<Block::Header>, String>
where
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
