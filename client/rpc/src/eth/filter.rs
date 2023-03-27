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

use std::{marker::PhantomData, sync::Arc, time};

use ethereum::BlockV2 as EthereumBlock;
use ethereum_types::{H256, U256};
use jsonrpsee::core::{async_trait, RpcResult as Result};
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, NumberFor, One, Saturating, UniqueSaturatedInto},
};
// Frontier
use fc_rpc_core::{types::*, EthFilterApiServer};
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};

use crate::{eth::cache::EthBlockDataCacheTask, frontier_backend_client, internal_err};

pub struct EthFilter<B: BlockT, C, BE> {
	client: Arc<C>,
	backend: Arc<fc_db::Backend<B>>,
	filter_pool: FilterPool,
	max_stored_filters: usize,
	max_past_logs: u32,
	block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, C, BE> EthFilter<B, C, BE> {
	pub fn new(
		client: Arc<C>,
		backend: Arc<fc_db::Backend<B>>,
		filter_pool: FilterPool,
		max_stored_filters: usize,
		max_past_logs: u32,
		block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	) -> Self {
		Self {
			client,
			backend,
			filter_pool,
			max_stored_filters,
			max_past_logs,
			block_data_cache,
			_marker: PhantomData,
		}
	}
}

impl<B, C, BE> EthFilter<B, C, BE>
where
	B: BlockT,
	C: HeaderBackend<B>,
{
	fn create_filter(&self, filter_type: FilterType) -> Result<U256> {
		let block_number =
			UniqueSaturatedInto::<u64>::unique_saturated_into(self.client.info().best_number);
		let pool = self.filter_pool.clone();
		let response = if let Ok(locked) = &mut pool.lock() {
			if locked.len() >= self.max_stored_filters {
				return Err(internal_err(format!(
					"Filter pool is full (limit {:?}).",
					self.max_stored_filters
				)));
			}
			let last_key = match {
				let mut iter = locked.iter();
				iter.next_back()
			} {
				Some((k, _)) => *k,
				None => U256::zero(),
			};
			// Assume `max_stored_filters` is always < U256::max.
			let key = last_key.checked_add(U256::one()).unwrap();
			locked.insert(
				key,
				FilterPoolItem {
					last_poll: BlockNumber::Num(block_number),
					filter_type,
					at_block: block_number,
				},
			);
			Ok(key)
		} else {
			Err(internal_err("Filter pool is not available."))
		};
		response
	}
}

#[async_trait]
impl<B, C, BE> EthFilterApiServer for EthFilter<B, C, BE>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
{
	fn new_filter(&self, filter: Filter) -> Result<U256> {
		self.create_filter(FilterType::Log(filter))
	}

	fn new_block_filter(&self) -> Result<U256> {
		self.create_filter(FilterType::Block)
	}

	fn new_pending_transaction_filter(&self) -> Result<U256> {
		Err(internal_err("Method not available."))
	}

	async fn filter_changes(&self, index: Index) -> Result<FilterChanges> {
		// There are multiple branches that needs to return async blocks.
		// Also, each branch need to (synchronously) do stuff with the pool
		// (behind a lock), and the lock should be released before entering
		// an async block.
		//
		// To avoid issues with multiple async blocks (having different
		// anonymous types) we collect all necessary data in this enum then have
		// a single async block.
		enum FuturePath<B: BlockT> {
			Block {
				last: u64,
				next: u64,
			},
			Log {
				filter: Filter,
				from_number: NumberFor<B>,
				current_number: NumberFor<B>,
			},
			Error(jsonrpsee::core::Error),
		}

		let key = U256::from(index.value());
		let block_number =
			UniqueSaturatedInto::<u64>::unique_saturated_into(self.client.info().best_number);
		let pool = self.filter_pool.clone();
		// Try to lock.
		let path = if let Ok(locked) = &mut pool.lock() {
			// Try to get key.
			if let Some(pool_item) = locked.get(&key).cloned() {
				match &pool_item.filter_type {
					// For each block created since last poll, get a vector of ethereum hashes.
					FilterType::Block => {
						let last = pool_item.last_poll.to_min_block_num().unwrap();
						let next = block_number + 1;
						// Update filter `last_poll`.
						locked.insert(
							key,
							FilterPoolItem {
								last_poll: BlockNumber::Num(next),
								filter_type: pool_item.filter_type.clone(),
								at_block: pool_item.at_block,
							},
						);

						FuturePath::<B>::Block { last, next }
					}
					// For each event since last poll, get a vector of ethereum logs.
					FilterType::Log(filter) => {
						// Update filter `last_poll`.
						locked.insert(
							key,
							FilterPoolItem {
								last_poll: BlockNumber::Num(block_number + 1),
								filter_type: pool_item.filter_type.clone(),
								at_block: pool_item.at_block,
							},
						);

						// Either the filter-specific `to` block or best block.
						let best_number = self.client.info().best_number;
						let mut current_number = filter
							.to_block
							.and_then(|v| v.to_min_block_num())
							.map(|s| s.unique_saturated_into())
							.unwrap_or(best_number);

						if current_number > best_number {
							current_number = best_number;
						}

						// The from clause is the max(last_poll, filter_from).
						let last_poll = pool_item
							.last_poll
							.to_min_block_num()
							.unwrap()
							.unique_saturated_into();

						let filter_from = filter
							.from_block
							.and_then(|v| v.to_min_block_num())
							.map(|s| s.unique_saturated_into())
							.unwrap_or(last_poll);

						let from_number = std::cmp::max(last_poll, filter_from);

						// Build the response.
						FuturePath::Log {
							filter: filter.clone(),
							from_number,
							current_number,
						}
					}
					// Should never reach here.
					_ => FuturePath::Error(internal_err("Method not available.")),
				}
			} else {
				FuturePath::Error(internal_err(format!("Filter id {:?} does not exist.", key)))
			}
		} else {
			FuturePath::Error(internal_err("Filter pool is not available."))
		};

		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let max_past_logs = self.max_past_logs;

		match path {
			FuturePath::Error(err) => Err(err),
			FuturePath::Block { last, next } => {
				let mut ethereum_hashes: Vec<H256> = Vec::new();
				for n in last..next {
					let id = BlockId::Number(n.unique_saturated_into());
					let substrate_hash = client.expect_block_hash_from_id(&id).map_err(|_| {
						internal_err(format!("Expect block number from id: {}", id))
					})?;

					let schema =
						fc_storage::onchain_storage_schema(client.as_ref(), substrate_hash);

					let block = block_data_cache.current_block(schema, substrate_hash).await;
					if let Some(block) = block {
						ethereum_hashes.push(block.header.hash())
					}
				}
				Ok(FilterChanges::Hashes(ethereum_hashes))
			}
			FuturePath::Log {
				filter,
				from_number,
				current_number,
			} => {
				let mut ret: Vec<Log> = Vec::new();
				let _ = filter_range_logs(
					client.as_ref(),
					&block_data_cache,
					&mut ret,
					max_past_logs,
					&filter,
					from_number,
					current_number,
				)
				.await?;

				Ok(FilterChanges::Logs(ret))
			}
		}
	}

	async fn filter_logs(&self, index: Index) -> Result<Vec<Log>> {
		let key = U256::from(index.value());
		let pool = self.filter_pool.clone();

		// We want to get the filter, while releasing the pool lock outside
		// of the async block.
		let filter_result: Result<Filter> = (|| {
			let pool = pool
				.lock()
				.map_err(|_| internal_err("Filter pool is not available."))?;

			let pool_item = pool
				.get(&key)
				.ok_or_else(|| internal_err(format!("Filter id {:?} does not exist.", key)))?;

			match &pool_item.filter_type {
				FilterType::Log(filter) => Ok(filter.clone()),
				_ => Err(internal_err(format!(
					"Filter id {:?} is not a Log filter.",
					key
				))),
			}
		})();

		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let max_past_logs = self.max_past_logs;

		let filter = filter_result?;

		let best_number = client.info().best_number;
		let mut current_number = filter
			.to_block
			.and_then(|v| v.to_min_block_num())
			.map(|s| s.unique_saturated_into())
			.unwrap_or(best_number);

		if current_number > best_number {
			current_number = best_number;
		}

		let from_number = filter
			.from_block
			.and_then(|v| v.to_min_block_num())
			.map(|s| s.unique_saturated_into())
			.unwrap_or(best_number);

		let mut ret: Vec<Log> = Vec::new();
		let _ = filter_range_logs(
			client.as_ref(),
			&block_data_cache,
			&mut ret,
			max_past_logs,
			&filter,
			from_number,
			current_number,
		)
		.await?;
		Ok(ret)
	}

	fn uninstall_filter(&self, index: Index) -> Result<bool> {
		let key = U256::from(index.value());
		let pool = self.filter_pool.clone();
		// Try to lock.
		let response = if let Ok(locked) = &mut pool.lock() {
			if locked.remove(&key).is_some() {
				Ok(true)
			} else {
				Err(internal_err(format!("Filter id {:?} does not exist.", key)))
			}
		} else {
			Err(internal_err("Filter pool is not available."))
		};
		response
	}

	async fn logs(&self, filter: Filter) -> Result<Vec<Log>> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);
		let max_past_logs = self.max_past_logs;

		let mut ret: Vec<Log> = Vec::new();
		if let Some(hash) = filter.block_hash {
			let substrate_hash = match frontier_backend_client::load_hash::<B, C>(
				client.as_ref(),
				backend.as_ref(),
				hash,
			)
			.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(Vec::new()),
			};
			let schema = fc_storage::onchain_storage_schema(client.as_ref(), substrate_hash);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;
			if let (Some(block), Some(statuses)) = (block, statuses) {
				filter_block_logs(&mut ret, &filter, block, statuses);
			}
		} else {
			let best_number = client.info().best_number;
			let mut current_number = filter
				.to_block
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(best_number);

			if current_number > best_number {
				current_number = best_number;
			}

			let from_number = filter
				.from_block
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(best_number);

			let _ = filter_range_logs(
				client.as_ref(),
				&block_data_cache,
				&mut ret,
				max_past_logs,
				&filter,
				from_number,
				current_number,
			)
			.await?;
		}
		Ok(ret)
	}
}

async fn filter_range_logs<B: BlockT, C, BE>(
	client: &C,
	block_data_cache: &EthBlockDataCacheTask<B>,
	ret: &mut Vec<Log>,
	max_past_logs: u32,
	filter: &Filter,
	from: NumberFor<B>,
	to: NumberFor<B>,
) -> Result<()>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
{
	// Max request duration of 10 seconds.
	let max_duration = time::Duration::from_secs(10);
	let begin_request = time::Instant::now();

	let mut current_number = from;

	// Pre-calculate BloomInput for reuse.
	let topics_input = if filter.topics.is_some() {
		let filtered_params = FilteredParams::new(Some(filter.clone()));
		Some(filtered_params.flat_topics)
	} else {
		None
	};
	let address_bloom_filter = FilteredParams::adresses_bloom_filter(&filter.address);
	let topics_bloom_filter = FilteredParams::topics_bloom_filter(&topics_input);

	while current_number <= to {
		let id = BlockId::Number(current_number);
		let substrate_hash = client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

		let schema = fc_storage::onchain_storage_schema(client, substrate_hash);

		let block = block_data_cache.current_block(schema, substrate_hash).await;

		if let Some(block) = block {
			if FilteredParams::address_in_bloom(block.header.logs_bloom, &address_bloom_filter)
				&& FilteredParams::topics_in_bloom(block.header.logs_bloom, &topics_bloom_filter)
			{
				let statuses = block_data_cache
					.current_transaction_statuses(schema, substrate_hash)
					.await;
				if let Some(statuses) = statuses {
					filter_block_logs(ret, filter, block, statuses);
				}
			}
		}
		// Check for restrictions
		if ret.len() as u32 > max_past_logs {
			return Err(internal_err(format!(
				"query returned more than {} results",
				max_past_logs
			)));
		}
		if begin_request.elapsed() > max_duration {
			return Err(internal_err(format!(
				"query timeout of {} seconds exceeded",
				max_duration.as_secs()
			)));
		}
		if current_number == to {
			break;
		} else {
			current_number = current_number.saturating_add(One::one());
		}
	}
	Ok(())
}

fn filter_block_logs<'a>(
	ret: &'a mut Vec<Log>,
	filter: &'a Filter,
	block: EthereumBlock,
	transaction_statuses: Vec<TransactionStatus>,
) -> &'a Vec<Log> {
	let params = FilteredParams::new(Some(filter.clone()));
	let mut block_log_index: u32 = 0;
	let block_hash = H256::from(keccak_256(&rlp::encode(&block.header)));
	for status in transaction_statuses.iter() {
		let logs = status.logs.clone();
		let mut transaction_log_index: u32 = 0;
		let transaction_hash = status.transaction_hash;
		for ethereum_log in logs {
			let mut log = Log {
				address: ethereum_log.address,
				topics: ethereum_log.topics.clone(),
				data: Bytes(ethereum_log.data.clone()),
				block_hash: None,
				block_number: None,
				transaction_hash: None,
				transaction_index: None,
				log_index: None,
				transaction_log_index: None,
				removed: false,
			};
			let mut add: bool = true;
			match (filter.address.clone(), filter.topics.clone()) {
				(Some(_), Some(_)) => {
					if !params.filter_address(&log) || !params.filter_topics(&log) {
						add = false;
					}
				}
				(Some(_), _) => {
					if !params.filter_address(&log) {
						add = false;
					}
				}
				(_, Some(_)) => {
					if !params.filter_topics(&log) {
						add = false;
					}
				}
				_ => {}
			}
			if add {
				log.block_hash = Some(block_hash);
				log.block_number = Some(block.header.number);
				log.transaction_hash = Some(transaction_hash);
				log.transaction_index = Some(U256::from(status.transaction_index));
				log.log_index = Some(U256::from(block_log_index));
				log.transaction_log_index = Some(U256::from(transaction_log_index));
				ret.push(log);
			}
			transaction_log_index += 1;
			block_log_index += 1;
		}
	}
	ret
}
