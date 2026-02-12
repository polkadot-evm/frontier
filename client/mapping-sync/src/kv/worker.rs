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

use std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration};

use futures::{
	prelude::*,
	task::{Context, Poll},
};
use futures_timer::Delay;
use log::debug;
// Substrate
use sc_client_api::{
	backend::{Backend, StorageProvider},
	client::ImportNotifications,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
// Frontier
use fc_storage::StorageOverride;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{ReorgInfo, SyncStrategy};

/// Information tracked at import time for a block that was `is_new_best`.
pub struct BestBlockInfo<Block: BlockT> {
	/// The block number (for pruning purposes).
	pub block_number: <Block::Header as HeaderT>::Number,
	/// Reorg info if this block became best as part of a reorganization.
	pub reorg_info: Option<Arc<ReorgInfo<Block>>>,
}

pub struct MappingSyncWorker<Block: BlockT, C, BE> {
	import_notifications: ImportNotifications<Block>,
	timeout: Duration,
	inner_delay: Option<Delay>,

	client: Arc<C>,
	substrate_backend: Arc<BE>,
	storage_override: Arc<dyn StorageOverride<Block>>,
	frontier_backend: Arc<fc_db::kv::Backend<Block, C>>,

	have_next: bool,
	retry_times: usize,
	sync_from: <Block::Header as HeaderT>::Number,
	strategy: SyncStrategy,

	sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
	pubsub_notification_sinks:
		Arc<crate::EthereumBlockNotificationSinks<crate::EthereumBlockNotification<Block>>>,

	/// Tracks block hashes that were `is_new_best` at the time of their import notification,
	/// along with their block number for pruning purposes and optional reorg info.
	/// This is used to correctly determine `is_new_best` when syncing blocks, avoiding race
	/// conditions where the best hash may have changed between import and sync time.
	/// Entries are pruned when blocks become finalized to prevent unbounded growth.
	best_at_import: HashMap<Block::Hash, BestBlockInfo<Block>>,
}

impl<Block: BlockT, C, BE> Unpin for MappingSyncWorker<Block, C, BE> {}

impl<Block: BlockT, C, BE> MappingSyncWorker<Block, C, BE> {
	pub fn new(
		import_notifications: ImportNotifications<Block>,
		timeout: Duration,
		client: Arc<C>,
		substrate_backend: Arc<BE>,
		storage_override: Arc<dyn StorageOverride<Block>>,
		frontier_backend: Arc<fc_db::kv::Backend<Block, C>>,
		retry_times: usize,
		sync_from: <Block::Header as HeaderT>::Number,
		strategy: SyncStrategy,
		sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
		pubsub_notification_sinks: Arc<
			crate::EthereumBlockNotificationSinks<crate::EthereumBlockNotification<Block>>,
		>,
	) -> Self {
		Self {
			import_notifications,
			timeout,
			inner_delay: None,

			client,
			substrate_backend,
			storage_override,
			frontier_backend,

			have_next: true,
			retry_times,
			sync_from,
			strategy,

			sync_oracle,
			pubsub_notification_sinks,
			best_at_import: HashMap::new(),
		}
	}
}

impl<Block, C, BE> Stream for MappingSyncWorker<Block, C, BE>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>,
	C::Api: EthereumRuntimeRPCApi<Block>,
	C: HeaderBackend<Block> + StorageProvider<Block, BE>,
	BE: Backend<Block>,
{
	type Item = ();

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<()>> {
		let mut fire = false;

		loop {
			match Stream::poll_next(Pin::new(&mut self.import_notifications), cx) {
				Poll::Pending => break,
				Poll::Ready(Some(notification)) => {
					fire = true;
					// Track blocks that were `is_new_best` at import time to avoid race
					// conditions when determining `is_new_best` at sync time.
					// We store the block number to enable pruning of old entries,
					// and reorg info if this block became best as part of a reorg.
					if notification.is_new_best {
						// For notification: include new_best_hash per Ethereum spec.
						let reorg_info = notification.tree_route.as_ref().map(|tree_route| {
							Arc::new(ReorgInfo::from_tree_route(tree_route, notification.hash))
						});
						self.best_at_import.insert(
							notification.hash,
							BestBlockInfo {
								block_number: *notification.header.number(),
								reorg_info,
							},
						);
					}
				}
				Poll::Ready(None) => return Poll::Ready(None),
			}
		}

		let timeout = self.timeout;
		let inner_delay = self.inner_delay.get_or_insert_with(|| Delay::new(timeout));

		match Future::poll(Pin::new(inner_delay), cx) {
			Poll::Pending => (),
			Poll::Ready(()) => {
				fire = true;
			}
		}

		if self.have_next {
			fire = true;
		}

		if fire {
			self.inner_delay = None;

			// Temporarily take ownership of best_at_import to avoid borrow checker issues
			// (we can't have both an immutable borrow of self.client and a mutable borrow
			// of self.best_at_import at the same time)
			let mut best_at_import = std::mem::take(&mut self.best_at_import);

			let result = crate::kv::sync_blocks(
				self.client.as_ref(),
				self.substrate_backend.as_ref(),
				self.storage_override.clone(),
				self.frontier_backend.as_ref(),
				self.retry_times,
				self.sync_from,
				self.strategy,
				self.sync_oracle.clone(),
				self.pubsub_notification_sinks.clone(),
				&mut best_at_import,
			);

			// Restore the best_at_import set
			self.best_at_import = best_at_import;

			if let Err(e) = crate::kv::repair_canonical_number_mappings_batch(
				self.client.as_ref(),
				self.storage_override.as_ref(),
				self.frontier_backend.as_ref(),
				self.sync_from,
				crate::kv::CANONICAL_NUMBER_REPAIR_BATCH_SIZE,
			) {
				debug!(
					target: "mapping-sync",
					"Canonical number mapping repair failed with error {e:?}, retrying."
				);
			}

			match result {
				Ok(have_next) => {
					self.have_next = have_next;
					Poll::Ready(Some(()))
				}
				Err(e) => {
					self.have_next = false;
					debug!(target: "mapping-sync", "Syncing failed with error {e:?}, retrying.");
					Poll::Ready(Some(()))
				}
			}
		} else {
			Poll::Pending
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{EthereumBlockNotification, EthereumBlockNotificationSinks};
	use fc_storage::SchemaV3StorageOverride;
	use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};
	use sc_block_builder::BlockBuilderBuilder;
	use sc_client_api::BlockchainEvents;
	use scale_codec::Encode;
	use sp_consensus::BlockOrigin;
	use sp_core::{H160, H256, U256};
	use sp_runtime::{generic::Header, traits::BlakeTwo256, Digest};
	use substrate_test_runtime_client::{
		ClientBlockImportExt, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	use tempfile::tempdir;

	type OpaqueBlock = sp_runtime::generic::Block<
		Header<u64, BlakeTwo256>,
		substrate_test_runtime_client::runtime::Extrinsic,
	>;

	fn ethereum_digest() -> Digest {
		let partial_header = ethereum::PartialHeader {
			parent_hash: H256::random(),
			beneficiary: H160::default(),
			state_root: H256::default(),
			receipts_root: H256::default(),
			logs_bloom: ethereum_types::Bloom::default(),
			difficulty: U256::zero(),
			number: U256::zero(),
			gas_limit: U256::zero(),
			gas_used: U256::zero(),
			timestamp: 0u64,
			extra_data: Vec::new(),
			mix_hash: H256::default(),
			nonce: ethereum_types::H64::default(),
		};
		let ethereum_block = ethereum::Block::new(partial_header, vec![], vec![]);
		Digest {
			logs: vec![sp_runtime::generic::DigestItem::Consensus(
				fp_consensus::FRONTIER_ENGINE_ID,
				fp_consensus::PostLog::Hashes(fp_consensus::Hashes::from_block(ethereum_block))
					.encode(),
			)],
		}
	}

	struct TestSyncOracleNotSyncing;
	impl sp_consensus::SyncOracle for TestSyncOracleNotSyncing {
		fn is_major_syncing(&self) -> bool {
			false
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	struct TestSyncOracleSyncing;
	impl sp_consensus::SyncOracle for TestSyncOracleSyncing {
		fn is_major_syncing(&self) -> bool {
			true
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	#[tokio::test]
	async fn block_import_notification_works() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let test_sync_oracle = TestSyncOracleNotSyncing {};
		// Backend
		let backend = builder.backend();
		// Client
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let client = Arc::new(client);
		// Overrides
		let storage_override = Arc::new(SchemaV3StorageOverride::new(client.clone()));

		let frontier_backend = Arc::new(
			fc_db::kv::Backend::<OpaqueBlock, _>::new(
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
			.expect("frontier backend"),
		);

		let notification_stream = client.clone().import_notification_stream();
		let client_inner = client.clone();

		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		tokio::task::spawn(async move {
			MappingSyncWorker::new(
				notification_stream,
				Duration::new(6, 0),
				client_inner,
				backend,
				storage_override.clone(),
				frontier_backend,
				3,
				0,
				SyncStrategy::Normal,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.for_each(|()| future::ready(()))
			.await
		});

		{
			// A new mpsc channel
			let (inner_sink, mut block_notification_stream) =
				sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);

			{
				// This scope represents a call to eth_subscribe, where it briefly locks the pool
				// to push the new sink.
				let sinks = &mut pubsub_notification_sinks.lock();
				// Push to sink pool
				sinks.push(inner_sink);
			}

			// Let's produce a block, which we expect to trigger a channel message
			let chain_info = client.chain_info();
			let builder = BlockBuilderBuilder::new(&*client)
				.on_parent_block(chain_info.best_hash)
				.with_parent_block_number(chain_info.best_number)
				.with_inherent_digests(ethereum_digest())
				.build()
				.unwrap();
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			let _res = client.import(BlockOrigin::Own, block).await;

			// Receive
			assert_eq!(
				block_notification_stream
					.next()
					.await
					.expect("a message")
					.hash,
				block_hash
			);
		}

		{
			// Assert we still hold a sink in the pool after switching scopes
			let sinks = pubsub_notification_sinks.lock();
			assert_eq!(sinks.len(), 1);
		}

		{
			// Create yet another mpsc channel
			let (inner_sink, mut block_notification_stream) =
				sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);

			{
				let sinks = &mut pubsub_notification_sinks.lock();
				// Push it
				sinks.push(inner_sink);
				// Now we expect two sinks in the pool
				assert_eq!(sinks.len(), 2);
			}

			// Let's produce another block, this not only triggers a message in the new channel
			// but also removes the closed channels from the pool.
			let chain_info = client.chain_info();
			let builder = BlockBuilderBuilder::new(&*client)
				.on_parent_block(chain_info.best_hash)
				.with_parent_block_number(chain_info.best_number)
				.with_inherent_digests(ethereum_digest())
				.build()
				.unwrap();
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			let _res = client.import(BlockOrigin::Own, block).await;

			// Receive
			assert_eq!(
				block_notification_stream
					.next()
					.await
					.expect("a message")
					.hash,
				block_hash
			);

			// So we expect the pool to hold one sink only after cleanup
			let sinks = &mut pubsub_notification_sinks.lock();
			assert_eq!(sinks.len(), 1);
		}
	}

	#[tokio::test]
	async fn sink_removal_when_syncing_works() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let test_sync_oracle = TestSyncOracleSyncing {};
		// Backend
		let backend = builder.backend();
		// Client
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let client = Arc::new(client);
		// Overrides
		let storage_override = Arc::new(SchemaV3StorageOverride::new(client.clone()));

		let frontier_backend = Arc::new(
			fc_db::kv::Backend::<OpaqueBlock, _>::new(
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
			.expect("frontier backend"),
		);

		let notification_stream = client.clone().import_notification_stream();
		let client_inner = client.clone();

		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		tokio::task::spawn(async move {
			MappingSyncWorker::new(
				notification_stream,
				Duration::new(6, 0),
				client_inner,
				backend,
				storage_override.clone(),
				frontier_backend,
				3,
				0,
				SyncStrategy::Normal,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.for_each(|()| future::ready(()))
			.await
		});

		{
			// A new mpsc channel
			let (inner_sink, mut block_notification_stream) =
				sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);

			{
				// This scope represents a call to eth_subscribe, where it briefly locks the pool
				// to push the new sink.
				let sinks = &mut pubsub_notification_sinks.lock();
				// Push to sink pool
				sinks.push(inner_sink);
			}

			// Let's produce a block, which we expect to trigger a channel message
			let chain_info = client.chain_info();
			let builder = BlockBuilderBuilder::new(&*client)
				.on_parent_block(chain_info.best_hash)
				.with_parent_block_number(chain_info.best_number)
				.with_inherent_digests(ethereum_digest())
				.build()
				.unwrap();
			let block = builder.build().unwrap().block;
			let _res = client.import(BlockOrigin::Own, block).await;

			// Not received, channel closed because major syncing
			assert!(block_notification_stream.next().await.is_none());
		}

		{
			// Assert sink was removed from pool on major syncing
			let sinks = pubsub_notification_sinks.lock();
			assert_eq!(sinks.len(), 0);
		}
	}

	#[tokio::test]
	async fn sync_block_can_skip_number_mapping_write() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let client = Arc::new(client);
		let frontier_backend = Arc::new(
			fc_db::kv::Backend::<OpaqueBlock, _>::new(
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
			.expect("frontier backend"),
		);

		let first_hash = H256::repeat_byte(0xAA);
		let second_hash = H256::repeat_byte(0xBB);
		let first_commitment = fc_db::kv::MappingCommitment::<OpaqueBlock> {
			block_hash: H256::repeat_byte(0x01),
			ethereum_block_hash: first_hash,
			ethereum_transaction_hashes: vec![],
		};
		let second_commitment = fc_db::kv::MappingCommitment::<OpaqueBlock> {
			block_hash: H256::repeat_byte(0x02),
			ethereum_block_hash: second_hash,
			ethereum_transaction_hashes: vec![],
		};

		frontier_backend
			.mapping()
			.write_hashes(first_commitment, 1, fc_db::kv::NumberMappingWrite::Write)
			.expect("write first mapping");
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1).expect("read number"),
			Some(first_hash)
		);
		frontier_backend
			.mapping()
			.write_hashes(second_commitment, 1, fc_db::kv::NumberMappingWrite::Skip)
			.expect("write second mapping");
		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(1).expect("read number"),
			Some(first_hash)
		);

		// Keep backend alive in this scope.
		drop(backend);
	}

	#[tokio::test]
	async fn repair_batch_advances_cursor_when_runtime_block_is_unavailable() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let client = Arc::new(client);
		let storage_override = Arc::new(SchemaV3StorageOverride::new(client.clone()));
		let frontier_backend = Arc::new(
			fc_db::kv::Backend::<OpaqueBlock, _>::new(
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
			.expect("frontier backend"),
		);

		frontier_backend
			.mapping()
			.set_block_hash_by_number(0, H256::repeat_byte(0x11))
			.expect("seed stale mapping");
		assert_eq!(
			frontier_backend.mapping().canonical_number_repair_cursor(),
			Ok(None)
		);

		crate::kv::repair_canonical_number_mappings_batch(
			client.as_ref(),
			storage_override.as_ref(),
			frontier_backend.as_ref(),
			0,
			16,
		)
		.expect("repair batch");

		assert_eq!(
			frontier_backend.mapping().block_hash_by_number(0),
			Ok(Some(H256::repeat_byte(0x11)))
		);
		assert_eq!(
			frontier_backend.mapping().canonical_number_repair_cursor(),
			Ok(Some(0))
		);

		drop(backend);
	}
}
