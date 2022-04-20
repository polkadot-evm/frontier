// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2022 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::{
	collections::{BTreeMap, HashMap},
	marker::PhantomData,
	sync::{Arc, Mutex},
};

use ethereum::BlockV2 as EthereumBlock;
use ethereum_types::{H256, U256};
use futures::StreamExt;
use lru::LruCache;
use tokio::sync::{mpsc, oneshot};

use codec::Decode;
use sc_client_api::{
	backend::{Backend, StateBackend, StorageProvider},
	client::BlockchainEvents,
};
use sc_service::SpawnTaskHandle;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT, Header as HeaderT, UniqueSaturatedInto, Zero},
};

use fc_rpc_core::types::*;
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};
use fp_storage::EthereumStorageSchema;

use crate::{
	frontier_backend_client,
	overrides::{OverrideHandle, StorageOverride},
};

enum EthBlockDataCacheMessage<B: BlockT> {
	RequestCurrentBlock {
		block_hash: B::Hash,
		schema: EthereumStorageSchema,
		response_tx: oneshot::Sender<Option<EthereumBlock>>,
	},
	FetchedCurrentBlock {
		block_hash: B::Hash,
		block: Option<EthereumBlock>,
	},

	RequestCurrentTransactionStatuses {
		block_hash: B::Hash,
		schema: EthereumStorageSchema,
		response_tx: oneshot::Sender<Option<Vec<TransactionStatus>>>,
	},
	FetchedCurrentTransactionStatuses {
		block_hash: B::Hash,
		statuses: Option<Vec<TransactionStatus>>,
	},
}

/// Manage LRU caches for block data and their transaction statuses.
/// These are large and take a lot of time to fetch from the database.
/// Storing them in an LRU cache will allow to reduce database accesses
/// when many subsequent requests are related to the same blocks.
pub struct EthBlockDataCache<B: BlockT>(mpsc::Sender<EthBlockDataCacheMessage<B>>);

impl<B: BlockT> EthBlockDataCache<B> {
	pub fn new(
		spawn_handle: SpawnTaskHandle,
		overrides: Arc<OverrideHandle<B>>,
		blocks_cache_size: usize,
		statuses_cache_size: usize,
	) -> Self {
		let (task_tx, mut task_rx) = mpsc::channel(100);
		let outer_task_tx = task_tx.clone();
		let outer_spawn_handle = spawn_handle.clone();

		outer_spawn_handle.spawn("EthBlockDataCache", None, async move {
			let mut blocks_cache = LruCache::<B::Hash, EthereumBlock>::new(blocks_cache_size);
			let mut statuses_cache =
				LruCache::<B::Hash, Vec<TransactionStatus>>::new(statuses_cache_size);

			let mut awaiting_blocks =
				HashMap::<B::Hash, Vec<oneshot::Sender<Option<EthereumBlock>>>>::new();
			let mut awaiting_statuses =
				HashMap::<B::Hash, Vec<oneshot::Sender<Option<Vec<TransactionStatus>>>>>::new();

			// Handle all incoming messages.
			// Exits when there are no more senders.
			// Any long computation should be spawned in a separate task
			// to keep this task handle messages as soon as possible.
			while let Some(message) = task_rx.recv().await {
				use EthBlockDataCacheMessage::*;
				match message {
					RequestCurrentBlock {
						block_hash,
						schema,
						response_tx,
					} => Self::request_current(
						&spawn_handle,
						&mut blocks_cache,
						&mut awaiting_blocks,
						Arc::clone(&overrides),
						block_hash,
						schema,
						response_tx,
						task_tx.clone(),
						move |handler| FetchedCurrentBlock {
							block_hash,
							block: handler.current_block(&BlockId::Hash(block_hash)),
						},
					),
					FetchedCurrentBlock { block_hash, block } => {
						if let Some(wait_list) = awaiting_blocks.remove(&block_hash) {
							for sender in wait_list {
								let _ = sender.send(block.clone());
							}
						}

						if let Some(block) = block {
							blocks_cache.put(block_hash, block);
						}
					}

					RequestCurrentTransactionStatuses {
						block_hash,
						schema,
						response_tx,
					} => Self::request_current(
						&spawn_handle,
						&mut statuses_cache,
						&mut awaiting_statuses,
						Arc::clone(&overrides),
						block_hash,
						schema,
						response_tx,
						task_tx.clone(),
						move |handler| FetchedCurrentTransactionStatuses {
							block_hash,
							statuses: handler
								.current_transaction_statuses(&BlockId::Hash(block_hash)),
						},
					),
					FetchedCurrentTransactionStatuses {
						block_hash,
						statuses,
					} => {
						if let Some(wait_list) = awaiting_statuses.remove(&block_hash) {
							for sender in wait_list {
								let _ = sender.send(statuses.clone());
							}
						}

						if let Some(statuses) = statuses {
							statuses_cache.put(block_hash, statuses);
						}
					}
				}
			}
		});

		Self(outer_task_tx)
	}

	fn request_current<T, F>(
		spawn_handle: &SpawnTaskHandle,
		cache: &mut LruCache<B::Hash, T>,
		wait_list: &mut HashMap<B::Hash, Vec<oneshot::Sender<Option<T>>>>,
		overrides: Arc<OverrideHandle<B>>,
		block_hash: B::Hash,
		schema: EthereumStorageSchema,
		response_tx: oneshot::Sender<Option<T>>,
		task_tx: mpsc::Sender<EthBlockDataCacheMessage<B>>,
		handler_call: F,
	) where
		T: Clone,
		F: FnOnce(&Box<dyn StorageOverride<B> + Send + Sync>) -> EthBlockDataCacheMessage<B>,
		F: Send + 'static,
	{
		// Data is cached, we respond immediately.
		if let Some(data) = cache.get(&block_hash).cloned() {
			let _ = response_tx.send(Some(data));
			return;
		}

		// Another request already triggered caching but the
		// response is not known yet, we add the sender to the waiting
		// list.
		if let Some(waiting) = wait_list.get_mut(&block_hash) {
			waiting.push(response_tx);
			return;
		}

		// Data is neither cached nor already requested, so we start fetching
		// the data.
		wait_list.insert(block_hash.clone(), vec![response_tx]);

		spawn_handle.spawn("EthBlockDataCache Worker", None, async move {
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let message = handler_call(handler);
			let _ = task_tx.send(message).await;
		});
	}

	/// Cache for `handler.current_block`.
	pub async fn current_block(
		&self,
		schema: EthereumStorageSchema,
		block_hash: B::Hash,
	) -> Option<EthereumBlock> {
		let (response_tx, response_rx) = oneshot::channel();

		let _ = self
			.0
			.send(EthBlockDataCacheMessage::RequestCurrentBlock {
				block_hash,
				schema,
				response_tx,
			})
			.await
			.ok()?;

		response_rx.await.ok()?
	}

	/// Cache for `handler.current_transaction_statuses`.
	pub async fn current_transaction_statuses(
		&self,
		schema: EthereumStorageSchema,
		block_hash: B::Hash,
	) -> Option<Vec<TransactionStatus>> {
		let (response_tx, response_rx) = oneshot::channel();

		let _ = self
			.0
			.send(
				EthBlockDataCacheMessage::RequestCurrentTransactionStatuses {
					block_hash,
					schema,
					response_tx,
				},
			)
			.await
			.ok()?;

		response_rx.await.ok()?
	}
}

pub struct EthTask<B, C, BE>(PhantomData<(B, C, BE)>);

impl<B, C, BE> EthTask<B, C, BE>
where
	B: BlockT<Hash = H256>,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + BlockchainEvents<B>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	/// Task that caches at which best hash a new EthereumStorageSchema was inserted in the Runtime Storage.
	pub async fn ethereum_schema_cache_task(client: Arc<C>, backend: Arc<fc_db::Backend<B>>) {
		use fp_storage::PALLET_ETHEREUM_SCHEMA;
		use log::warn;
		use sp_storage::{StorageData, StorageKey};

		if let Ok(None) = frontier_backend_client::load_cached_schema::<B>(backend.as_ref()) {
			let mut cache: Vec<(EthereumStorageSchema, H256)> = Vec::new();
			let id = BlockId::Number(Zero::zero());
			if let Ok(Some(header)) = client.header(id) {
				let genesis_schema_version = frontier_backend_client::onchain_storage_schema::<
					B,
					C,
					BE,
				>(client.as_ref(), id);
				cache.push((genesis_schema_version, header.hash()));
				let _ = frontier_backend_client::write_cached_schema::<B>(backend.as_ref(), cache)
					.map_err(|err| {
						warn!("Error schema cache insert for genesis: {:?}", err);
					});
			} else {
				warn!("Error genesis header unreachable");
			}
		}

		// Subscribe to changes for the pallet-ethereum Schema.
		if let Ok(mut stream) = client.storage_changes_notification_stream(
			Some(&[StorageKey(PALLET_ETHEREUM_SCHEMA.to_vec())]),
			None,
		) {
			while let Some(notification) = stream.next().await {
				let (hash, changes) = (notification.block, notification.changes);
				// Make sure only block hashes marked as best are referencing cache checkpoints.
				if hash == client.info().best_hash {
					// Just map the change set to the actual data.
					let storage: Vec<Option<StorageData>> = changes
						.iter()
						.filter_map(|(o_sk, _k, v)| {
							if o_sk.is_none() {
								Some(v.cloned())
							} else {
								None
							}
						})
						.collect();
					for change in storage {
						if let Some(data) = change {
							// Decode the wrapped blob which's type is known.
							let new_schema: EthereumStorageSchema =
								Decode::decode(&mut &data.0[..]).unwrap();
							// Cache new entry and overwrite the old database value.
							if let Ok(Some(old_cache)) =
								frontier_backend_client::load_cached_schema::<B>(backend.as_ref())
							{
								let mut new_cache: Vec<(EthereumStorageSchema, H256)> = old_cache;
								match &new_cache[..] {
									[.., (schema, _)] if *schema == new_schema => {
										warn!(
											"Schema version already in Frontier database, ignoring: {:?}",
											new_schema
										);
									}
									_ => {
										new_cache.push((new_schema, hash));
										let _ = frontier_backend_client::write_cached_schema::<B>(
											backend.as_ref(),
											new_cache,
										)
										.map_err(|err| {
											warn!(
												"Error schema cache insert for genesis: {:?}",
												err
											);
										});
									}
								}
							} else {
								warn!("Error schema cache is corrupted");
							}
						}
					}
				}
			}
		}
	}

	pub async fn filter_pool_task(
		client: Arc<C>,
		filter_pool: Arc<Mutex<BTreeMap<U256, FilterPoolItem>>>,
		retain_threshold: u64,
	) {
		let mut notification_st = client.import_notification_stream();

		while let Some(notification) = notification_st.next().await {
			if let Ok(filter_pool) = &mut filter_pool.lock() {
				let imported_number: u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(
					*notification.header.number(),
				);

				// BTreeMap::retain is unstable :c.
				// 1. We collect all keys to remove.
				// 2. We remove them.
				let remove_list: Vec<_> = filter_pool
					.iter()
					.filter_map(|(&k, v)| {
						let lifespan_limit = v.at_block + retain_threshold;
						if lifespan_limit <= imported_number {
							Some(k)
						} else {
							None
						}
					})
					.collect();

				for key in remove_list {
					filter_pool.remove(&key);
				}
			}
		}
	}

	pub async fn fee_history_task(
		client: Arc<C>,
		overrides: Arc<OverrideHandle<B>>,
		fee_history_cache: FeeHistoryCache,
		block_limit: u64,
	) {
		use sp_runtime::Permill;

		struct TransactionHelper {
			gas_used: u64,
			effective_reward: u64,
		}
		// Calculates the cache for a single block
		#[rustfmt::skip]
			let fee_history_cache_item = |hash: H256, elasticity: Permill| -> (
			FeeHistoryCacheItem,
			Option<u64>
		) {
			let id = BlockId::Hash(hash);
			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			// Evenly spaced percentile list from 0.0 to 100.0 with a 0.5 resolution.
			// This means we cache 200 percentile points.
			// Later in request handling we will approximate by rounding percentiles that
			// fall in between with `(round(n*2)/2)`.
			let reward_percentiles: Vec<f64> = {
				let mut percentile: f64 = 0.0;
				(0..201)
					.into_iter()
					.map(|_| {
						let val = percentile;
						percentile += 0.5;
						val
					})
					.collect()
			};

			let block = handler.current_block(&id);
			let mut block_number: Option<u64> = None;
			let base_fee = if let Some(base_fee) = handler.base_fee(&id) {
				base_fee
			} else {
				client.runtime_api().gas_price(&id).unwrap_or(U256::zero())
			};
			let receipts = handler.current_receipts(&id);
			let mut result = FeeHistoryCacheItem {
				base_fee: base_fee.as_u64(),
				gas_used_ratio: 0f64,
				rewards: Vec::new(),
			};
			if let (Some(block), Some(receipts)) = (block, receipts) {
				block_number = Some(block.header.number.as_u64());
				// Calculate the gas used ratio.
				// TODO this formula needs the pallet-base-fee configuration.
				// By now we assume just the default 0.125 (elasticity multiplier 8).
				let gas_used = block.header.gas_used.as_u64() as f64;
				let gas_limit = block.header.gas_limit.as_u64() as f64;
				let elasticity_multiplier: f64 = (elasticity / Permill::from_parts(1_000_000))
					.deconstruct()
					.into();
				let gas_target = gas_limit / elasticity_multiplier;

				result.gas_used_ratio = gas_used / (gas_target * elasticity_multiplier);

				let mut previous_cumulative_gas = U256::zero();
				let used_gas = |current: U256, previous: &mut U256| -> u64 {
					let r = current.saturating_sub(*previous).as_u64();
					*previous = current;
					r
				};
				// Build a list of relevant transaction information.
				let mut transactions: Vec<TransactionHelper> = receipts
					.iter()
					.enumerate()
					.map(|(i, receipt)| TransactionHelper {
						gas_used: match receipt {
							ethereum::ReceiptV3::Legacy(d) | ethereum::ReceiptV3::EIP2930(d) | ethereum::ReceiptV3::EIP1559(d) => used_gas(d.used_gas, &mut previous_cumulative_gas),
						},
						effective_reward: match block.transactions.get(i) {
							Some(&ethereum::TransactionV2::Legacy(ref t)) => {
								t.gas_price.saturating_sub(base_fee).as_u64()
							}
							Some(&ethereum::TransactionV2::EIP2930(ref t)) => {
								t.gas_price.saturating_sub(base_fee).as_u64()
							}
							Some(&ethereum::TransactionV2::EIP1559(ref t)) => t
								.max_priority_fee_per_gas
								.min(t.max_fee_per_gas.saturating_sub(base_fee))
								.as_u64(),
							None => 0,
						},
					})
					.collect();
				// Sort ASC by effective reward.
				transactions.sort_by(|a, b| a.effective_reward.cmp(&b.effective_reward));

				// Calculate percentile rewards.
				result.rewards = reward_percentiles
					.into_iter()
					.filter_map(|p| {
						let target_gas = (p * gas_used / 100f64) as u64;
						let mut sum_gas = 0;
						for tx in &transactions {
							sum_gas += tx.gas_used;
							if target_gas <= sum_gas {
								return Some(tx.effective_reward);
							}
						}
						None
					})
					.map(|r| r)
					.collect();
			} else {
				result.rewards = reward_percentiles.iter().map(|_| 0).collect();
			}
			(result, block_number)
		};

		// Commits the result to cache
		let commit_if_any = |item: FeeHistoryCacheItem, key: Option<u64>| {
			if let (Some(block_number), Ok(fee_history_cache)) =
				(key, &mut fee_history_cache.lock())
			{
				fee_history_cache.insert(block_number, item);
				// We want to remain within the configured cache bounds.
				// The first key out of bounds.
				let first_out = block_number.saturating_sub(block_limit);
				// Out of bounds size.
				let to_remove = (fee_history_cache.len() as u64).saturating_sub(block_limit);
				// Remove all cache data before `block_limit`.
				for i in 0..to_remove {
					// Cannot overflow.
					let key = first_out - i;
					fee_history_cache.remove(&key);
				}
			}
		};

		let mut notification_st = client.import_notification_stream();

		while let Some(notification) = notification_st.next().await {
			if notification.is_new_best {
				let hash = notification.hash;
				let id = BlockId::Hash(hash);
				let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
					client.as_ref(),
					id,
				);
				let handler = overrides
					.schemas
					.get(&schema)
					.unwrap_or(&overrides.fallback);

				let default_elasticity = Permill::from_parts(125_000);
				let elasticity = handler.elasticity(&id).unwrap_or(default_elasticity);
				// In case a re-org happened on import.
				if let Some(tree_route) = notification.tree_route {
					if let Ok(fee_history_cache) = &mut fee_history_cache.lock() {
						// Remove retracted.
						let _ = tree_route.retracted().iter().map(|hash_and_number| {
							let n = UniqueSaturatedInto::<u64>::unique_saturated_into(
								hash_and_number.number,
							);
							fee_history_cache.remove(&n);
						});
						// Insert enacted.
						let _ = tree_route.enacted().iter().map(|hash_and_number| {
							let (result, block_number) =
								fee_history_cache_item(hash_and_number.hash, elasticity);
							commit_if_any(result, block_number);
						});
					}
				}
				// Cache the imported block.
				let (result, block_number) = fee_history_cache_item(hash, elasticity);
				commit_if_any(result, block_number);
			}
		}
	}
}
