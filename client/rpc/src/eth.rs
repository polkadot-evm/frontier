// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
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
	time,
};

use ethereum::{BlockV2 as EthereumBlock, TransactionV2 as EthereumTransaction};
use ethereum_types::{H160, H256, H512, H64, U256, U64};
use evm::{ExitError, ExitReason};
use futures::{future::TryFutureExt, StreamExt};
use jsonrpc_core::{futures::future, BoxFuture, Result};
use lru::LruCache;
use tokio::sync::{mpsc, oneshot};

use codec::{Decode, Encode};
use sc_client_api::{
	backend::{Backend, StateBackend, StorageProvider},
	client::BlockchainEvents,
};
use sc_network::{ExHashT, NetworkService};
use sc_service::SpawnTaskHandle;
use sc_transaction_pool::{ChainApi, Pool};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::{ApiExt, BlockId, Core, HeaderT, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::{
	traits::{BlakeTwo256, Block as BlockT, NumberFor, One, Saturating, UniqueSaturatedInto, Zero},
	transaction_validity::TransactionSource,
};

use fc_rpc_core::{
	types::{
		Block, BlockNumber, BlockTransactions, Bytes, CallRequest, FeeHistory, FeeHistoryCache,
		FeeHistoryCacheItem, Filter, FilterChanges, FilterPool, FilterPoolItem, FilterType,
		FilteredParams, Header, Index, Log, Receipt, Rich, RichBlock, SyncInfo, SyncStatus,
		Transaction, TransactionMessage, TransactionRequest, Work,
	},
	EthApi as EthApiT, EthFilterApi as EthFilterApiT,
};
use fp_rpc::{ConvertTransactionRuntimeApi, EthereumRuntimeRPCApi, TransactionStatus};
use fp_storage::EthereumStorageSchema;

use crate::{
	error_on_execution_failure, frontier_backend_client, internal_err, overrides::OverrideHandle,
	public_key, EthSigner, StorageOverride,
};

pub struct EthApi<B: BlockT, C, P, CT, BE, H: ExHashT, A: ChainApi> {
	pool: Arc<P>,
	graph: Arc<Pool<A>>,
	client: Arc<C>,
	convert_transaction: Option<CT>,
	network: Arc<NetworkService<B, H>>,
	is_authority: bool,
	signers: Vec<Box<dyn EthSigner>>,
	overrides: Arc<OverrideHandle<B>>,
	backend: Arc<fc_db::Backend<B>>,
	max_past_logs: u32,
	block_data_cache: Arc<EthBlockDataCache<B>>,
	fee_history_limit: u64,
	fee_history_cache: FeeHistoryCache,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, C, P, CT, BE, H: ExHashT, A: ChainApi> EthApi<B, C, P, CT, BE, H, A> {
	pub fn new(
		client: Arc<C>,
		pool: Arc<P>,
		graph: Arc<Pool<A>>,
		convert_transaction: Option<CT>,
		network: Arc<NetworkService<B, H>>,
		signers: Vec<Box<dyn EthSigner>>,
		overrides: Arc<OverrideHandle<B>>,
		backend: Arc<fc_db::Backend<B>>,
		is_authority: bool,
		max_past_logs: u32,
		block_data_cache: Arc<EthBlockDataCache<B>>,
		fee_history_limit: u64,
		fee_history_cache: FeeHistoryCache,
	) -> Self {
		Self {
			client,
			pool,
			graph,
			convert_transaction,
			network,
			is_authority,
			signers,
			overrides,
			backend,
			max_past_logs,
			block_data_cache,
			fee_history_limit,
			fee_history_cache,
			_marker: PhantomData,
		}
	}
}

fn rich_block_build(
	block: ethereum::Block<EthereumTransaction>,
	statuses: Vec<Option<TransactionStatus>>,
	hash: Option<H256>,
	full_transactions: bool,
	base_fee: Option<U256>,
	is_eip1559: bool,
) -> RichBlock {
	Rich {
		inner: Block {
			header: Header {
				hash: Some(
					hash.unwrap_or_else(|| H256::from(keccak_256(&rlp::encode(&block.header)))),
				),
				parent_hash: block.header.parent_hash,
				uncles_hash: block.header.ommers_hash,
				author: block.header.beneficiary,
				miner: block.header.beneficiary,
				state_root: block.header.state_root,
				transactions_root: block.header.transactions_root,
				receipts_root: block.header.receipts_root,
				number: Some(block.header.number),
				gas_used: block.header.gas_used,
				gas_limit: block.header.gas_limit,
				extra_data: Bytes(block.header.extra_data.clone()),
				logs_bloom: block.header.logs_bloom,
				timestamp: U256::from(block.header.timestamp / 1000),
				difficulty: block.header.difficulty,
				seal_fields: vec![
					Bytes(block.header.mix_hash.as_bytes().to_vec()),
					Bytes(block.header.nonce.as_bytes().to_vec()),
				],
				size: Some(U256::from(rlp::encode(&block.header).len() as u32)),
			},
			total_difficulty: U256::zero(),
			uncles: vec![],
			transactions: {
				if full_transactions {
					BlockTransactions::Full(
						block
							.transactions
							.iter()
							.enumerate()
							.map(|(index, transaction)| {
								transaction_build(
									transaction.clone(),
									Some(block.clone()),
									Some(statuses[index].clone().unwrap_or_default()),
									is_eip1559,
									base_fee,
								)
							})
							.collect(),
					)
				} else {
					BlockTransactions::Hashes(
						block
							.transactions
							.iter()
							.map(|transaction| transaction.hash())
							.collect(),
					)
				}
			},
			size: Some(U256::from(rlp::encode(&block).len() as u32)),
			base_fee_per_gas: base_fee,
		},
		extra_info: BTreeMap::new(),
	}
}

fn transaction_build(
	ethereum_transaction: EthereumTransaction,
	block: Option<ethereum::Block<EthereumTransaction>>,
	status: Option<TransactionStatus>,
	is_eip1559: bool,
	base_fee: Option<U256>,
) -> Transaction {
	let mut transaction: Transaction = ethereum_transaction.clone().into();

	if let EthereumTransaction::EIP1559(_) = ethereum_transaction {
		if block.is_none() && status.is_none() {
			// If transaction is not mined yet, gas price is considered just max fee per gas.
			transaction.gas_price = transaction.max_fee_per_gas;
		} else {
			// If transaction is already mined, gas price is considered base fee + priority fee.
			// A.k.a. effective gas price.
			let base_fee = base_fee.unwrap_or(U256::zero());
			let max_priority_fee_per_gas =
				transaction.max_priority_fee_per_gas.unwrap_or(U256::zero());
			transaction.gas_price = Some(
				base_fee
					.checked_add(max_priority_fee_per_gas)
					.unwrap_or(U256::max_value()),
			);
		}
	} else if !is_eip1559 {
		// This is a pre-eip1559 support transaction a.k.a. txns on frontier before we introduced EIP1559 support in
		// pallet-ethereum schema V2.
		// They do not include `maxFeePerGas`, `maxPriorityFeePerGas` or `type` fields.
		transaction.max_fee_per_gas = None;
		transaction.max_priority_fee_per_gas = None;
		transaction.transaction_type = None;
	}

	let pubkey = match public_key(&ethereum_transaction) {
		Ok(p) => Some(p),
		Err(_e) => None,
	};

	// Block hash.
	transaction.block_hash = block.as_ref().map_or(None, |block| {
		Some(H256::from(keccak_256(&rlp::encode(&block.header))))
	});
	// Block number.
	transaction.block_number = block.as_ref().map(|block| block.header.number);
	// Transaction index.
	transaction.transaction_index = status.as_ref().map(|status| {
		U256::from(UniqueSaturatedInto::<u32>::unique_saturated_into(
			status.transaction_index,
		))
	});
	// From.
	transaction.from = status.as_ref().map_or(
		{
			match pubkey {
				Some(pk) => H160::from(H256::from(keccak_256(&pk))),
				_ => H160::default(),
			}
		},
		|status| status.from,
	);
	// To.
	transaction.to = status.as_ref().map_or(
		{
			let action = match ethereum_transaction {
				EthereumTransaction::Legacy(t) => t.action,
				EthereumTransaction::EIP2930(t) => t.action,
				EthereumTransaction::EIP1559(t) => t.action,
			};
			match action {
				ethereum::TransactionAction::Call(to) => Some(to),
				_ => None,
			}
		},
		|status| status.to,
	);
	// Creates.
	transaction.creates = status
		.as_ref()
		.map_or(None, |status| status.contract_address);
	// Public key.
	transaction.public_key = pubkey.as_ref().map(|pk| H512::from(pk));

	transaction
}

fn pending_runtime_api<'a, B: BlockT, C, BE, A: ChainApi>(
	client: &'a C,
	graph: &'a Pool<A>,
) -> Result<sp_api::ApiRef<'a, C::Api>>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: BlockBuilder<B> + EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	A: ChainApi<Block = B> + 'static,
{
	// In case of Pending, we need an overlayed state to query over.
	let api = client.runtime_api();
	let best = BlockId::Hash(client.info().best_hash);
	// Get all transactions in the ready queue.
	let xts: Vec<<B as BlockT>::Extrinsic> = graph
		.validated_pool()
		.ready()
		.map(|in_pool_tx| in_pool_tx.data().clone())
		.collect::<Vec<<B as BlockT>::Extrinsic>>();
	// Manually initialize the overlay.
	let header = client.header(best).unwrap().unwrap();
	api.initialize_block(&best, &header)
		.map_err(|e| internal_err(format!("Runtime api access error: {:?}", e)))?;
	// Apply the ready queue to the best block's state.
	for xt in xts {
		let _ = api.apply_extrinsic(&best, xt);
	}
	Ok(api)
}

async fn filter_range_logs<B: BlockT, C, BE>(
	client: &C,
	backend: &fc_db::Backend<B>,
	block_data_cache: &EthBlockDataCache<B>,
	ret: &mut Vec<Log>,
	max_past_logs: u32,
	filter: &Filter,
	from: NumberFor<B>,
	to: NumberFor<B>,
) -> Result<()>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	// Max request duration of 10 seconds.
	let max_duration = time::Duration::from_secs(10);
	let begin_request = time::Instant::now();

	let mut current_number = from;

	// Pre-calculate BloomInput for reuse.
	let topics_input = if let Some(_) = &filter.topics {
		let filtered_params = FilteredParams::new(Some(filter.clone()));
		Some(filtered_params.flat_topics)
	} else {
		None
	};
	let address_bloom_filter = FilteredParams::adresses_bloom_filter(&filter.address);
	let topics_bloom_filter = FilteredParams::topics_bloom_filter(&topics_input);

	// Get schema cache. A single read before the block range iteration.
	// This prevents having to do an extra DB read per block range iteration to getthe actual schema.
	let mut local_cache: BTreeMap<NumberFor<B>, EthereumStorageSchema> = BTreeMap::new();
	if let Ok(Some(schema_cache)) = frontier_backend_client::load_cached_schema::<B>(backend) {
		for (schema, hash) in schema_cache {
			if let Ok(Some(header)) = client.header(BlockId::Hash(hash)) {
				let number = *header.number();
				local_cache.insert(number, schema);
			}
		}
	}
	let cache_keys: Vec<NumberFor<B>> = local_cache.keys().cloned().collect();
	let mut default_schema: Option<&EthereumStorageSchema> = None;
	if cache_keys.len() == 1 {
		// There is only one schema and that's the one we use.
		default_schema = local_cache.get(&cache_keys[0]);
	}

	while current_number <= to {
		let id = BlockId::Number(current_number);
		let substrate_hash = client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

		let schema = match default_schema {
			// If there is a single schema, we just assign.
			Some(default_schema) => *default_schema,
			_ => {
				// If there are multiple schemas, we iterate over the - hopefully short - list
				// of keys and assign the one belonging to the current_number.
				// Because there are more than 1 schema, and current_number cannot be < 0,
				// (i - 1) will always be >= 0.
				let mut default_schema: Option<&EthereumStorageSchema> = None;
				for (i, k) in cache_keys.iter().enumerate() {
					if &current_number < k {
						default_schema = local_cache.get(&cache_keys[i - 1]);
					}
				}
				match default_schema {
					Some(schema) => *schema,
					// Fallback to DB read. This will happen i.e. when there is no cache
					// task configured at service level.
					_ => frontier_backend_client::onchain_storage_schema::<B, C, BE>(client, id),
				}
			}
		};

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
				address: ethereum_log.address.clone(),
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
			if let (Some(_), Some(_)) = (filter.address.clone(), filter.topics.clone()) {
				if !params.filter_address(&log) || !params.filter_topics(&log) {
					add = false;
				}
			} else if let Some(_) = filter.address {
				if !params.filter_address(&log) {
					add = false;
				}
			} else if let Some(_) = &filter.topics {
				if !params.filter_topics(&log) {
					add = false;
				}
			}
			if add {
				log.block_hash = Some(block_hash);
				log.block_number = Some(block.header.number.clone());
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

struct FeeDetails {
	gas_price: Option<U256>,
	max_fee_per_gas: Option<U256>,
	max_priority_fee_per_gas: Option<U256>,
}

fn fee_details(
	request_gas_price: Option<U256>,
	request_max_fee: Option<U256>,
	request_priority: Option<U256>,
) -> Result<FeeDetails> {
	match (request_gas_price, request_max_fee, request_priority) {
		(gas_price, None, None) => {
			// Legacy request, all default to gas price.
			Ok(FeeDetails {
				gas_price,
				max_fee_per_gas: gas_price,
				max_priority_fee_per_gas: gas_price,
			})
		}
		(_, max_fee, max_priority) => {
			// eip-1559
			// Ensure `max_priority_fee_per_gas` is less or equal to `max_fee_per_gas`.
			if let Some(max_priority) = max_priority {
				let max_fee = max_fee.unwrap_or_default();
				if max_priority > max_fee {
					return Err(internal_err(format!(
						"Invalid input: `max_priority_fee_per_gas` greater than `max_fee_per_gas`"
					)));
				}
			}
			Ok(FeeDetails {
				gas_price: max_fee,
				max_fee_per_gas: max_fee,
				max_priority_fee_per_gas: max_priority,
			})
		}
	}
}

impl<B, C, P, CT, BE, H: ExHashT, A> EthApiT for EthApi<B, C, P, CT, BE, H, A>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: BlockBuilder<B> + ConvertTransactionRuntimeApi<B> + EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	P: TransactionPool<Block = B> + Send + Sync + 'static,
	A: ChainApi<Block = B> + 'static,
	CT: fp_rpc::ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
{
	fn protocol_version(&self) -> Result<u64> {
		Ok(1)
	}

	fn syncing(&self) -> Result<SyncStatus> {
		if self.network.is_major_syncing() {
			let block_number = U256::from(UniqueSaturatedInto::<u128>::unique_saturated_into(
				self.client.info().best_number.clone(),
			));
			Ok(SyncStatus::Info(SyncInfo {
				starting_block: U256::zero(),
				current_block: block_number,
				// TODO `highest_block` is not correct, should load `best_seen_block` from NetworkWorker,
				// but afaik that is not currently possible in Substrate:
				// https://github.com/paritytech/substrate/issues/7311
				highest_block: block_number,
				warp_chunks_amount: None,
				warp_chunks_processed: None,
			}))
		} else {
			Ok(SyncStatus::None)
		}
	}

	fn hashrate(&self) -> Result<U256> {
		Ok(U256::zero())
	}

	fn author(&self) -> Result<H160> {
		let block = BlockId::Hash(self.client.info().best_hash);
		let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
			self.client.as_ref(),
			block,
		);

		Ok(self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(&block)
			.ok_or(internal_err("fetching author through override failed"))?
			.header
			.beneficiary)
	}

	fn is_mining(&self) -> Result<bool> {
		Ok(self.is_authority)
	}

	fn chain_id(&self) -> Result<Option<U64>> {
		let hash = self.client.info().best_hash;
		Ok(Some(
			self.client
				.runtime_api()
				.chain_id(&BlockId::Hash(hash))
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.into(),
		))
	}

	fn gas_price(&self) -> Result<U256> {
		let block = BlockId::Hash(self.client.info().best_hash);

		Ok(self
			.client
			.runtime_api()
			.gas_price(&block)
			.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
			.into())
	}

	fn accounts(&self) -> Result<Vec<H160>> {
		let mut accounts = Vec::new();
		for signer in &self.signers {
			accounts.append(&mut signer.accounts());
		}
		Ok(accounts)
	}

	fn block_number(&self) -> Result<U256> {
		Ok(U256::from(
			UniqueSaturatedInto::<u128>::unique_saturated_into(
				self.client.info().best_number.clone(),
			),
		))
	}

	fn balance(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		let number = number.unwrap_or(BlockNumber::Latest);
		if number == BlockNumber::Pending {
			let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
			return Ok(api
				.account_basic(&BlockId::Hash(self.client.info().best_hash), address)
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.balance
				.into());
		} else if let Ok(Some(id)) = frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		) {
			return Ok(self
				.client
				.runtime_api()
				.account_basic(&id, address)
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.balance
				.into());
		} else {
			Ok(U256::zero())
		}
	}

	fn storage_at(&self, address: H160, index: U256, number: Option<BlockNumber>) -> Result<H256> {
		let number = number.unwrap_or(BlockNumber::Latest);
		if number == BlockNumber::Pending {
			let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
			return Ok(api
				.storage_at(&BlockId::Hash(self.client.info().best_hash), address, index)
				.unwrap_or_default());
		} else if let Ok(Some(id)) = frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		) {
			let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
				self.client.as_ref(),
				id,
			);
			return Ok(self
				.overrides
				.schemas
				.get(&schema)
				.unwrap_or(&self.overrides.fallback)
				.storage_at(&id, address, index)
				.unwrap_or_default());
		} else {
			Ok(H256::default())
		}
	}

	fn block_by_hash(&self, hash: H256, full: bool) -> BoxFuture<Result<Option<RichBlock>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		Box::pin(async move {
			let id = match frontier_backend_client::load_hash::<B>(backend.as_ref(), hash)
				.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;

			let base_fee = handler.base_fee(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses) {
				(Some(block), Some(statuses)) => Ok(Some(rich_block_build(
					block,
					statuses.into_iter().map(|s| Some(s)).collect(),
					Some(hash),
					full,
					base_fee,
					is_eip1559,
				))),
				_ => Ok(None),
			}
		})
	}

	fn block_by_number(
		&self,
		number: BlockNumber,
		full: bool,
	) -> BoxFuture<Result<Option<RichBlock>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		Box::pin(async move {
			let id = match frontier_backend_client::native_block_id::<B, C>(
				client.as_ref(),
				backend.as_ref(),
				Some(number),
			)? {
				Some(id) => id,
				None => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;

			let base_fee = handler.base_fee(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses) {
				(Some(block), Some(statuses)) => {
					let hash = H256::from(keccak_256(&rlp::encode(&block.header)));

					Ok(Some(rich_block_build(
						block,
						statuses.into_iter().map(|s| Some(s)).collect(),
						Some(hash),
						full,
						base_fee,
						is_eip1559,
					)))
				}
				_ => Ok(None),
			}
		})
	}

	fn transaction_count(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		if let Some(BlockNumber::Pending) = number {
			let block = BlockId::Hash(self.client.info().best_hash);

			let nonce = self
				.client
				.runtime_api()
				.account_basic(&block, address)
				.map_err(|err| {
					internal_err(format!("fetch runtime account basic failed: {:?}", err))
				})?
				.nonce;

			let mut current_nonce = nonce;
			let mut current_tag = (address, nonce).encode();
			for tx in self.pool.ready() {
				// since transactions in `ready()` need to be ordered by nonce
				// it's fine to continue with current iterator.
				if tx.provides().get(0) == Some(&current_tag) {
					current_nonce = current_nonce.saturating_add(1.into());
					current_tag = (address, current_nonce).encode();
				}
			}

			return Ok(current_nonce);
		}

		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number,
		)? {
			Some(id) => id,
			None => return Ok(U256::zero()),
		};

		let nonce = self
			.client
			.runtime_api()
			.account_basic(&id, address)
			.map_err(|err| internal_err(format!("fetch runtime account basic failed: {:?}", err)))?
			.nonce
			.into();

		Ok(nonce)
	}

	fn block_transaction_count_by_hash(&self, hash: H256) -> Result<Option<U256>> {
		let id = match frontier_backend_client::load_hash::<B>(self.backend.as_ref(), hash)
			.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let schema =
			frontier_backend_client::onchain_storage_schema::<B, C, BE>(self.client.as_ref(), id);
		let block = self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(&id);

		match block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	fn block_transaction_count_by_number(&self, number: BlockNumber) -> Result<Option<U256>> {
		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		)? {
			Some(id) => id,
			None => return Ok(None),
		};
		let schema =
			frontier_backend_client::onchain_storage_schema::<B, C, BE>(self.client.as_ref(), id);
		let block = self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(&id);

		match block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	fn block_uncles_count_by_hash(&self, _: H256) -> Result<U256> {
		Ok(U256::zero())
	}

	fn block_uncles_count_by_number(&self, _: BlockNumber) -> Result<U256> {
		Ok(U256::zero())
	}

	fn code_at(&self, address: H160, number: Option<BlockNumber>) -> Result<Bytes> {
		let number = number.unwrap_or(BlockNumber::Latest);
		if number == BlockNumber::Pending {
			let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
			return Ok(api
				.account_code_at(&BlockId::Hash(self.client.info().best_hash), address)
				.unwrap_or(vec![])
				.into());
		} else if let Ok(Some(id)) = frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		) {
			let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
				self.client.as_ref(),
				id,
			);

			return Ok(self
				.overrides
				.schemas
				.get(&schema)
				.unwrap_or(&self.overrides.fallback)
				.account_code_at(&id, address)
				.unwrap_or(vec![])
				.into());
		} else {
			Ok(Bytes(vec![]))
		}
	}

	fn send_transaction(&self, request: TransactionRequest) -> BoxFuture<Result<H256>> {
		let from = match request.from {
			Some(from) => from,
			None => {
				let accounts = match self.accounts() {
					Ok(accounts) => accounts,
					Err(e) => return Box::pin(future::err(e)),
				};

				match accounts.get(0) {
					Some(account) => account.clone(),
					None => return Box::pin(future::err(internal_err("no signer available"))),
				}
			}
		};

		let nonce = match request.nonce {
			Some(nonce) => nonce,
			None => match self.transaction_count(from, None) {
				Ok(nonce) => nonce,
				Err(e) => return Box::pin(future::err(e)),
			},
		};

		let chain_id = match self.chain_id() {
			Ok(Some(chain_id)) => chain_id.as_u64(),
			Ok(None) => return Box::pin(future::err(internal_err("chain id not available"))),
			Err(e) => return Box::pin(future::err(e)),
		};

		let hash = self.client.info().best_hash;

		let gas_price = request.gas_price;
		let gas_limit = match request.gas {
			Some(gas_limit) => gas_limit,
			None => {
				let block = self
					.client
					.runtime_api()
					.current_block(&BlockId::Hash(hash));
				if let Ok(Some(block)) = block {
					block.header.gas_limit
				} else {
					return Box::pin(future::err(internal_err(format!(
						"block unavailable, cannot query gas limit"
					))));
				}
			}
		};
		let max_fee_per_gas = request.max_fee_per_gas;
		let message: Option<TransactionMessage> = request.into();
		let message = match message {
			Some(TransactionMessage::Legacy(mut m)) => {
				m.nonce = nonce;
				m.chain_id = Some(chain_id);
				m.gas_limit = gas_limit;
				if gas_price.is_none() {
					m.gas_price = self.gas_price().unwrap_or(U256::default());
				}
				TransactionMessage::Legacy(m)
			}
			Some(TransactionMessage::EIP2930(mut m)) => {
				m.nonce = nonce;
				m.chain_id = chain_id;
				m.gas_limit = gas_limit;
				if gas_price.is_none() {
					m.gas_price = self.gas_price().unwrap_or(U256::default());
				}
				TransactionMessage::EIP2930(m)
			}
			Some(TransactionMessage::EIP1559(mut m)) => {
				m.nonce = nonce;
				m.chain_id = chain_id;
				m.gas_limit = gas_limit;
				if max_fee_per_gas.is_none() {
					m.max_fee_per_gas = self.gas_price().unwrap_or(U256::default());
				}
				TransactionMessage::EIP1559(m)
			}
			_ => {
				return Box::pin(future::err(internal_err("invalid transaction parameters")));
			}
		};

		let mut transaction = None;

		for signer in &self.signers {
			if signer.accounts().contains(&from) {
				match signer.sign(message, &from) {
					Ok(t) => transaction = Some(t),
					Err(e) => return Box::pin(future::err(e)),
				}
				break;
			}
		}

		let transaction = match transaction {
			Some(transaction) => transaction,
			None => return Box::pin(future::err(internal_err("no signer available"))),
		};
		let transaction_hash = transaction.hash();

		let block_hash = BlockId::hash(self.client.info().best_hash);
		let api_version = match self
			.client
			.runtime_api()
			.api_version::<dyn ConvertTransactionRuntimeApi<B>>(&block_hash)
		{
			Ok(api_version) => api_version,
			_ => return Box::pin(future::err(internal_err("cannot access runtime api"))),
		};

		let extrinsic = match api_version {
			Some(2) => match self
				.client
				.runtime_api()
				.convert_transaction(&block_hash, transaction)
			{
				Ok(extrinsic) => extrinsic,
				Err(_) => return Box::pin(future::err(internal_err("cannot access runtime api"))),
			},
			Some(1) => {
				if let ethereum::TransactionV2::Legacy(legacy_transaction) = transaction {
					// To be compatible with runtimes that do not support transactions v2
					#[allow(deprecated)]
					match self
						.client
						.runtime_api()
						.convert_transaction_before_version_2(&block_hash, legacy_transaction)
					{
						Ok(extrinsic) => extrinsic,
						Err(_) => {
							return Box::pin(future::err(internal_err("cannot access runtime api")))
						}
					}
				} else {
					return Box::pin(future::err(internal_err(
						"This runtime not support eth transactions v2",
					)));
				}
			}
			None => {
				if let Some(ref convert_transaction) = self.convert_transaction {
					convert_transaction.convert_transaction(transaction.clone())
				} else {
					return Box::pin(future::err(internal_err(
						"No TransactionConverter is provided and the runtime api ConvertTransactionRuntimeApi is not found"
					)));
				}
			}
			_ => {
				return Box::pin(future::err(internal_err(
					"ConvertTransactionRuntimeApi version not supported",
				)))
			}
		};

		Box::pin(
			self.pool
				.submit_one(&block_hash, TransactionSource::Local, extrinsic)
				.map_ok(move |_| transaction_hash)
				.map_err(|err| {
					internal_err(format!("submit transaction to pool failed: {:?}", err))
				}),
		)
	}

	fn send_raw_transaction(&self, bytes: Bytes) -> BoxFuture<Result<H256>> {
		let slice = &bytes.0[..];
		if slice.len() == 0 {
			return Box::pin(future::err(internal_err("transaction data is empty")));
		}
		let first = slice.get(0).unwrap();
		let transaction = if first > &0x7f {
			// Legacy transaction. Decode and wrap in envelope.
			match rlp::decode::<ethereum::TransactionV0>(slice) {
				Ok(transaction) => ethereum::TransactionV2::Legacy(transaction),
				Err(_) => return Box::pin(future::err(internal_err("decode transaction failed"))),
			}
		} else {
			// Typed Transaction.
			// `ethereum` crate decode implementation for `TransactionV2` expects a valid rlp input,
			// and EIP-1559 breaks that assumption by prepending a version byte.
			// We re-encode the payload input to get a valid rlp, and the decode implementation will strip
			// them to check the transaction version byte.
			let extend = rlp::encode(&slice);
			match rlp::decode::<ethereum::TransactionV2>(&extend[..]) {
				Ok(transaction) => transaction,
				Err(_) => return Box::pin(future::err(internal_err("decode transaction failed"))),
			}
		};

		let transaction_hash = transaction.hash();

		let block_hash = BlockId::hash(self.client.info().best_hash);
		let api_version = match self
			.client
			.runtime_api()
			.api_version::<dyn ConvertTransactionRuntimeApi<B>>(&block_hash)
		{
			Ok(api_version) => api_version,
			_ => return Box::pin(future::err(internal_err("cannot access runtime api"))),
		};

		let extrinsic = match api_version {
			Some(2) => match self
				.client
				.runtime_api()
				.convert_transaction(&block_hash, transaction)
			{
				Ok(extrinsic) => extrinsic,
				Err(_) => return Box::pin(future::err(internal_err("cannot access runtime api"))),
			},
			Some(1) => {
				if let ethereum::TransactionV2::Legacy(legacy_transaction) = transaction {
					// To be compatible with runtimes that do not support transactions v2
					#[allow(deprecated)]
					match self
						.client
						.runtime_api()
						.convert_transaction_before_version_2(&block_hash, legacy_transaction)
					{
						Ok(extrinsic) => extrinsic,
						Err(_) => {
							return Box::pin(future::err(internal_err("cannot access runtime api")))
						}
					}
				} else {
					return Box::pin(future::err(internal_err(
						"This runtime not support eth transactions v2",
					)));
				}
			}
			None => {
				if let Some(ref convert_transaction) = self.convert_transaction {
					convert_transaction.convert_transaction(transaction.clone())
				} else {
					return Box::pin(future::err(internal_err(
					"No TransactionConverter is provided and the runtime api ConvertTransactionRuntimeApi is not found"
				)));
				}
			}
			_ => {
				return Box::pin(future::err(internal_err(
					"ConvertTransactionRuntimeApi version not supported",
				)))
			}
		};

		Box::pin(
			self.pool
				.submit_one(&block_hash, TransactionSource::Local, extrinsic)
				.map_ok(move |_| transaction_hash)
				.map_err(|err| {
					internal_err(format!("submit transaction to pool failed: {:?}", err))
				}),
		)
	}

	fn call(&self, request: CallRequest, number: Option<BlockNumber>) -> Result<Bytes> {
		let CallRequest {
			from,
			to,
			gas_price,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			gas,
			value,
			data,
			nonce,
			access_list,
			..
		} = request;

		let (gas_price, max_fee_per_gas, max_priority_fee_per_gas) = {
			let details = fee_details(gas_price, max_fee_per_gas, max_priority_fee_per_gas)?;
			(
				details.gas_price,
				details.max_fee_per_gas,
				details.max_priority_fee_per_gas,
			)
		};

		let (id, api) = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number,
		)? {
			Some(id) => (id, self.client.runtime_api()),
			None => {
				// Not mapped in the db, assume pending.
				let id = BlockId::Hash(self.client.info().best_hash);
				let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
				(id, api)
			}
		};

		let api_version =
			if let Ok(Some(api_version)) = api.api_version::<dyn EthereumRuntimeRPCApi<B>>(&id) {
				api_version
			} else {
				return Err(internal_err(format!(
					"failed to retrieve Runtime Api version"
				)));
			};
		// use given gas limit or query current block's limit
		let gas_limit = match gas {
			Some(amount) => amount,
			None => {
				let block = if api_version > 1 {
					api.current_block(&id)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
				} else {
					#[allow(deprecated)]
					let legacy_block = api.current_block_before_version_2(&id)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					if let Some(block) = legacy_block {
						Some(block.into())
					} else {
						None
					}
				};

				if let Some(block) = block {
					block.header.gas_limit
				} else {
					return Err(internal_err(format!(
						"block unavailable, cannot query gas limit"
					)));
				}
			}
		};
		let data = data.map(|d| d.0).unwrap_or_default();
		match to {
			Some(to) => {
				if api_version == 1 {
					// Legacy pre-london
					#[allow(deprecated)]
					let info = api.call_before_version_2(
						&id,
						from.unwrap_or_default(),
						to,
						data,
						value.unwrap_or_default(),
						gas_limit,
						gas_price,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else if api_version >= 2 && api_version < 4 {
					// Post-london
					#[allow(deprecated)]
					let info = api.call_before_version_4(
						&id,
						from.unwrap_or_default(),
						to,
						data,
						value.unwrap_or_default(),
						gas_limit,
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else if api_version == 4 {
					// Post-london + access list support
					let access_list = access_list.unwrap_or_default();
					let info = api
						.call(
							&id,
							from.unwrap_or_default(),
							to,
							data,
							value.unwrap_or_default(),
							gas_limit,
							max_fee_per_gas,
							max_priority_fee_per_gas,
							nonce,
							false,
							Some(
								access_list
									.into_iter()
									.map(|item| (item.address, item.slots))
									.collect(),
							),
						)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
						.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else {
					return Err(internal_err(format!(
						"failed to retrieve Runtime Api version"
					)));
				}
			}
			None => {
				if api_version == 1 {
					// Legacy pre-london
					#[allow(deprecated)]
					let info = api.create_before_version_2(
						&id,
						from.unwrap_or_default(),
						data,
						value.unwrap_or_default(),
						gas_limit,
						gas_price,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(&id, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else if api_version >= 2 && api_version < 4 {
					// Post-london
					#[allow(deprecated)]
					let info = api.create_before_version_4(
						&id,
						from.unwrap_or_default(),
						data,
						value.unwrap_or_default(),
						gas_limit,
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(&id, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else if api_version == 4 {
					// Post-london + access list support
					let access_list = access_list.unwrap_or_default();
					let info = api
						.create(
							&id,
							from.unwrap_or_default(),
							data,
							value.unwrap_or_default(),
							gas_limit,
							max_fee_per_gas,
							max_priority_fee_per_gas,
							nonce,
							false,
							Some(
								access_list
									.into_iter()
									.map(|item| (item.address, item.slots))
									.collect(),
							),
						)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
						.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(&id, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else {
					return Err(internal_err(format!(
						"failed to retrieve Runtime Api version"
					)));
				}
			}
		}
	}

	fn estimate_gas(
		&self,
		request: CallRequest,
		_: Option<BlockNumber>,
	) -> BoxFuture<Result<U256>> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);

		Box::pin(async move {
			// Define the lower bound of estimate
			const MIN_GAS_PER_TX: U256 = U256([21_000, 0, 0, 0]);

			// Get best hash (TODO missing support for estimating gas historically)
			let best_hash = client.info().best_hash;

			// For simple transfer to simple account, return MIN_GAS_PER_TX directly
			let is_simple_transfer = match &request.data {
				None => true,
				Some(vec) => vec.0.is_empty(),
			};
			if is_simple_transfer {
				if let Some(to) = request.to {
					let to_code = client
						.runtime_api()
						.account_code_at(&BlockId::Hash(best_hash), to)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					if to_code.is_empty() {
						return Ok(MIN_GAS_PER_TX);
					}
				}
			}

			let (gas_price, max_fee_per_gas, max_priority_fee_per_gas) = {
				let details = fee_details(
					request.gas_price,
					request.max_fee_per_gas,
					request.max_priority_fee_per_gas,
				)?;
				(
					details.gas_price,
					details.max_fee_per_gas,
					details.max_priority_fee_per_gas,
				)
			};

			let get_current_block_gas_limit = || async {
				let substrate_hash = client.info().best_hash;
				let id = BlockId::Hash(substrate_hash);
				let schema =
					frontier_backend_client::onchain_storage_schema::<B, C, BE>(&client, id);
				let block = block_data_cache.current_block(schema, substrate_hash).await;
				if let Some(block) = block {
					Ok(block.header.gas_limit)
				} else {
					return Err(internal_err("block unavailable, cannot query gas limit"));
				}
			};

			// Determine the highest possible gas limits
			let mut highest = match request.gas {
				Some(gas) => gas,
				None => {
					// query current block's gas limit
					get_current_block_gas_limit().await?
				}
			};

			let api = client.runtime_api();

			// Recap the highest gas allowance with account's balance.
			if let Some(from) = request.from {
				let gas_price = gas_price.unwrap_or_default();
				if gas_price > U256::zero() {
					let balance = api
						.account_basic(&BlockId::Hash(best_hash), from)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
						.balance;
					let mut available = balance;
					if let Some(value) = request.value {
						if value > available {
							return Err(internal_err("insufficient funds for transfer"));
						}
						available -= value;
					}
					let allowance = available / gas_price;
					if highest > allowance {
						log::warn!(
						"Gas estimation capped by limited funds original {} balance {} sent {} feecap {} fundable {}",
						highest,
						balance,
						request.value.unwrap_or_default(),
						gas_price,
						allowance
					);
						highest = allowance;
					}
				}
			}

			struct ExecutableResult {
				data: Vec<u8>,
				exit_reason: ExitReason,
				used_gas: U256,
			}

			// Create a helper to check if a gas allowance results in an executable transaction.
			//
			// A new ApiRef instance needs to be used per execution to avoid the overlayed state to affect
			// the estimation result of subsequent calls.
			//
			// Note that this would have a performance penalty if we introduce gas estimation for past
			// blocks - and thus, past runtime versions. Substrate has a default `runtime_cache_size` of
			// 2 slots LRU-style, meaning if users were to access multiple runtime versions in a short period
			// of time, the RPC response time would degrade a lot, as the VersionedRuntime needs to be compiled.
			//
			// To solve that, and if we introduce historical gas estimation, we'd need to increase that default.
			#[rustfmt::skip]
			let executable = move |
				request, gas_limit, api_version, api: sp_api::ApiRef<'_, C::Api>, estimate_mode
			| -> Result<ExecutableResult> {
				let CallRequest {
					from,
					to,
					gas,
					value,
					data,
					nonce,
					access_list,
					..
				} = request;

				// Use request gas limit only if it less than gas_limit parameter
				let gas_limit = core::cmp::min(gas.unwrap_or(gas_limit), gas_limit);

				let data = data.map(|d| d.0).unwrap_or_default();

				let (exit_reason, data, used_gas) = match to {
					Some(to) => {
						let info = if api_version == 1 {
							// Legacy pre-london
							#[allow(deprecated)]
							api.call_before_version_2(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								gas_price,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else if api_version < 4 {
							// Post-london
							#[allow(deprecated)]
							api.call_before_version_4(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							api.call(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
								Some(
									access_list
										.into_iter()
										.map(|item| (item.address, item.slots))
										.collect(),
								),
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						};

						(info.exit_reason, info.value, info.used_gas)
					}
					None => {
						let info = if api_version == 1 {
							// Legacy pre-london
							#[allow(deprecated)]
							api.create_before_version_2(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								gas_price,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else if api_version < 4 {
							// Post-london
							#[allow(deprecated)]
							api.create_before_version_4(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							api.create(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
								Some(
									access_list
										.into_iter()
										.map(|item| (item.address, item.slots))
										.collect(),
								),
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						};

						(info.exit_reason, Vec::new(), info.used_gas)
					}
				};
				Ok(ExecutableResult {
					exit_reason,
					data,
					used_gas,
				})
			};
			let api_version = if let Ok(Some(api_version)) =
				client
					.runtime_api()
					.api_version::<dyn EthereumRuntimeRPCApi<B>>(&BlockId::Hash(best_hash))
			{
				api_version
			} else {
				return Err(internal_err(format!(
					"failed to retrieve Runtime Api version"
				)));
			};

			// Verify that the transaction succeed with highest capacity
			let cap = highest;
			let estimate_mode = true;
			let ExecutableResult {
				data,
				exit_reason,
				used_gas,
			} = executable(
				request.clone(),
				highest,
				api_version,
				client.runtime_api(),
				estimate_mode,
			)?;
			match exit_reason {
				ExitReason::Succeed(_) => (),
				ExitReason::Error(ExitError::OutOfGas) => {
					return Err(internal_err(format!(
						"gas required exceeds allowance {}",
						cap
					)))
				}
				// If the transaction reverts, there are two possible cases,
				// it can revert because the called contract feels that it does not have enough
				// gas left to continue, or it can revert for another reason unrelated to gas.
				ExitReason::Revert(revert) => {
					if request.gas.is_some() || request.gas_price.is_some() {
						// If the user has provided a gas limit or a gas price, then we have executed
						// with less block gas limit, so we must reexecute with block gas limit to
						// know if the revert is due to a lack of gas or not.
						let ExecutableResult {
							data,
							exit_reason,
							used_gas: _,
						} = executable(
							request.clone(),
							get_current_block_gas_limit().await?,
							api_version,
							client.runtime_api(),
							estimate_mode,
						)?;
						match exit_reason {
							ExitReason::Succeed(_) => {
								return Err(internal_err(format!(
									"gas required exceeds allowance {}",
									cap
								)))
							}
							// The execution has been done with block gas limit, so it is not a lack of gas from the user.
							other => error_on_execution_failure(&other, &data)?,
						}
					} else {
						// The execution has already been done with block gas limit, so it is not a lack of gas from the user.
						error_on_execution_failure(&ExitReason::Revert(revert), &data)?
					}
				}
				other => error_on_execution_failure(&other, &data)?,
			};

			#[cfg(not(feature = "rpc_binary_search_estimate"))]
			{
				Ok(used_gas)
			}
			#[cfg(feature = "rpc_binary_search_estimate")]
			{
				// On binary search, evm estimate mode is disabled
				let estimate_mode = false;
				// Define the lower bound of the binary search
				let mut lowest = MIN_GAS_PER_TX;

				// Start close to the used gas for faster binary search
				let mut mid = std::cmp::min(used_gas * 3, (highest + lowest) / 2);

				// Execute the binary search and hone in on an executable gas limit.
				let mut previous_highest = highest;
				while (highest - lowest) > U256::one() {
					let ExecutableResult {
						data,
						exit_reason,
						used_gas: _,
					} = executable(
						request.clone(),
						mid,
						api_version,
						client.runtime_api(),
						estimate_mode,
					)?;
					match exit_reason {
						ExitReason::Succeed(_) => {
							highest = mid;
							// If the variation in the estimate is less than 10%,
							// then the estimate is considered sufficiently accurate.
							if (previous_highest - highest) * 10 / previous_highest < U256::one() {
								return Ok(highest);
							}
							previous_highest = highest;
						}
						ExitReason::Revert(_) | ExitReason::Error(ExitError::OutOfGas) => {
							lowest = mid;
						}
						other => error_on_execution_failure(&other, &data)?,
					}
					mid = (highest + lowest) / 2;
				}

				Ok(highest)
			}
		})
	}

	fn transaction_by_hash(&self, hash: H256) -> BoxFuture<Result<Option<Transaction>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);
		let graph = Arc::clone(&self.graph);

		Box::pin(async move {
			let (hash, index) = match frontier_backend_client::load_transactions::<B, C>(
				client.as_ref(),
				backend.as_ref(),
				hash,
				true,
			)
			.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some((hash, index)) => (hash, index as usize),
				None => {
					let api = client.runtime_api();
					let best_block: BlockId<B> = BlockId::Hash(client.info().best_hash);

					let api_version = if let Ok(Some(api_version)) =
						api.api_version::<dyn EthereumRuntimeRPCApi<B>>(&best_block)
					{
						api_version
					} else {
						return Err(internal_err(format!(
							"failed to retrieve Runtime Api version"
						)));
					};
					// If the transaction is not yet mapped in the frontier db,
					// check for it in the transaction pool.
					let mut xts: Vec<<B as BlockT>::Extrinsic> = Vec::new();
					// Collect transactions in the ready validated pool.
					xts.extend(
						graph
							.validated_pool()
							.ready()
							.map(|in_pool_tx| in_pool_tx.data().clone())
							.collect::<Vec<<B as BlockT>::Extrinsic>>(),
					);

					// Collect transactions in the future validated pool.
					xts.extend(
						graph
							.validated_pool()
							.futures()
							.iter()
							.map(|(_hash, extrinsic)| extrinsic.clone())
							.collect::<Vec<<B as BlockT>::Extrinsic>>(),
					);

					let ethereum_transactions: Vec<EthereumTransaction> = if api_version > 1 {
						api.extrinsic_filter(&best_block, xts).map_err(|err| {
							internal_err(format!(
								"fetch runtime extrinsic filter failed: {:?}",
								err
							))
						})?
					} else {
						#[allow(deprecated)]
						let legacy = api.extrinsic_filter_before_version_2(&best_block, xts)
							.map_err(|err| {
								internal_err(format!(
									"fetch runtime extrinsic filter failed: {:?}",
									err
								))
							})?;
						legacy.into_iter().map(|tx| tx.into()).collect()
					};

					for txn in ethereum_transactions {
						let inner_hash = txn.hash();
						if hash == inner_hash {
							return Ok(Some(transaction_build(txn, None, None, true, None)));
						}
					}
					// Unknown transaction.
					return Ok(None);
				}
			};

			let id = match frontier_backend_client::load_hash::<B>(backend.as_ref(), hash)
				.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;

			let base_fee = handler.base_fee(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses) {
				(Some(block), Some(statuses)) => Ok(Some(transaction_build(
					block.transactions[index].clone(),
					Some(block),
					Some(statuses[index].clone()),
					is_eip1559,
					base_fee,
				))),
				_ => Ok(None),
			}
		})
	}

	fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> BoxFuture<Result<Option<Transaction>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		Box::pin(async move {
			let id = match frontier_backend_client::load_hash::<B>(backend.as_ref(), hash)
				.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let index = index.value();

			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;

			let base_fee = handler.base_fee(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses) {
				(Some(block), Some(statuses)) => {
					if let (Some(transaction), Some(status)) =
						(block.transactions.get(index), statuses.get(index))
					{
						return Ok(Some(transaction_build(
							transaction.clone(),
							Some(block),
							Some(status.clone()),
							is_eip1559,
							base_fee,
						)));
					} else {
						return Err(internal_err(format!("{:?} is out of bounds", index)));
					}
				}
				_ => Ok(None),
			}
		})
	}

	fn transaction_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> BoxFuture<Result<Option<Transaction>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		Box::pin(async move {
			let id = match frontier_backend_client::native_block_id::<B, C>(
				client.as_ref(),
				backend.as_ref(),
				Some(number),
			)? {
				Some(id) => id,
				None => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let index = index.value();
			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;

			let base_fee = handler.base_fee(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses) {
				(Some(block), Some(statuses)) => {
					if let (Some(transaction), Some(status)) =
						(block.transactions.get(index), statuses.get(index))
					{
						return Ok(Some(transaction_build(
							transaction.clone(),
							Some(block),
							Some(status.clone()),
							is_eip1559,
							base_fee,
						)));
					} else {
						return Err(internal_err(format!("{:?} is out of bounds", index)));
					}
				}
				_ => Ok(None),
			}
		})
	}

	fn transaction_receipt(&self, hash: H256) -> BoxFuture<Result<Option<Receipt>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		Box::pin(async move {
			let (hash, index) = match frontier_backend_client::load_transactions::<B, C>(
				client.as_ref(),
				backend.as_ref(),
				hash,
				true,
			)
			.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some((hash, index)) => (hash, index as usize),
				None => return Ok(None),
			};

			let id = match frontier_backend_client::load_hash::<B>(backend.as_ref(), hash)
				.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;
			let receipts = handler.current_receipts(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses, receipts) {
				(Some(block), Some(statuses), Some(receipts)) => {
					let block_hash = H256::from(keccak_256(&rlp::encode(&block.header)));
					let receipt = receipts[index].clone();

					let (logs, logs_bloom, status_code, cumulative_gas_used, gas_used) =
						if !is_eip1559 {
							// Pre-london frontier update stored receipts require cumulative gas calculation.
							match receipt {
								ethereum::ReceiptV3::Legacy(d) => {
									let index = core::cmp::min(receipts.len(), index + 1);
									let cumulative_gas: u32 = receipts[..index]
										.iter()
										.map(|r| match r {
											ethereum::ReceiptV3::Legacy(d) => {
												Ok(d.used_gas.as_u32())
											}
											_ => Err(internal_err(format!(
												"Unknown receipt for request {}",
												hash
											))),
										})
										.sum::<Result<u32>>()?;
									(
										d.logs,
										d.logs_bloom,
										d.status_code,
										U256::from(cumulative_gas),
										d.used_gas,
									)
								}
								_ => {
									return Err(internal_err(format!(
										"Unknown receipt for request {}",
										hash
									)))
								}
							}
						} else {
							match receipt {
								ethereum::ReceiptV3::Legacy(d)
								| ethereum::ReceiptV3::EIP2930(d)
								| ethereum::ReceiptV3::EIP1559(d) => {
									let cumulative_gas = d.used_gas;
									let gas_used = if index > 0 {
										let previous_receipt = receipts[index - 1].clone();
										let previous_gas_used = match previous_receipt {
											ethereum::ReceiptV3::Legacy(d)
											| ethereum::ReceiptV3::EIP2930(d)
											| ethereum::ReceiptV3::EIP1559(d) => d.used_gas,
										};
										cumulative_gas.saturating_sub(previous_gas_used)
									} else {
										cumulative_gas
									};
									(
										d.logs,
										d.logs_bloom,
										d.status_code,
										cumulative_gas,
										gas_used,
									)
								}
							}
						};

					let status = statuses[index].clone();
					let mut cumulative_receipts = receipts.clone();
					cumulative_receipts.truncate((status.transaction_index + 1) as usize);

					let transaction = block.transactions[index].clone();
					let effective_gas_price = match transaction {
						EthereumTransaction::Legacy(t) => t.gas_price,
						EthereumTransaction::EIP2930(t) => t.gas_price,
						EthereumTransaction::EIP1559(t) => handler
							.base_fee(&id)
							.unwrap_or_default()
							.checked_add(t.max_priority_fee_per_gas)
							.unwrap_or(U256::max_value()),
					};

					return Ok(Some(Receipt {
						transaction_hash: Some(status.transaction_hash),
						transaction_index: Some(status.transaction_index.into()),
						block_hash: Some(block_hash),
						from: Some(status.from),
						to: status.to,
						block_number: Some(block.header.number),
						cumulative_gas_used,
						gas_used: Some(gas_used),
						contract_address: status.contract_address,
						logs: {
							let mut pre_receipts_log_index = None;
							if cumulative_receipts.len() > 0 {
								cumulative_receipts.truncate(cumulative_receipts.len() - 1);
								pre_receipts_log_index = Some(
									cumulative_receipts
										.iter()
										.map(|r| match r {
											ethereum::ReceiptV3::Legacy(d)
											| ethereum::ReceiptV3::EIP2930(d)
											| ethereum::ReceiptV3::EIP1559(d) => d.logs.len() as u32,
										})
										.sum::<u32>(),
								);
							}
							logs.iter()
								.enumerate()
								.map(|(i, log)| Log {
									address: log.address,
									topics: log.topics.clone(),
									data: Bytes(log.data.clone()),
									block_hash: Some(block_hash),
									block_number: Some(block.header.number),
									transaction_hash: Some(status.transaction_hash),
									transaction_index: Some(status.transaction_index.into()),
									log_index: Some(U256::from(
										(pre_receipts_log_index.unwrap_or(0)) + i as u32,
									)),
									transaction_log_index: Some(U256::from(i)),
									removed: false,
								})
								.collect()
						},
						status_code: Some(U64::from(status_code)),
						logs_bloom,
						state_root: None,
						effective_gas_price,
					}));
				}
				_ => Ok(None),
			}
		})
	}

	fn uncle_by_block_hash_and_index(&self, _: H256, _: Index) -> Result<Option<RichBlock>> {
		Ok(None)
	}

	fn uncle_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> Result<Option<RichBlock>> {
		Ok(None)
	}

	fn logs(&self, filter: Filter) -> BoxFuture<Result<Vec<Log>>> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);
		let max_past_logs = self.max_past_logs;

		Box::pin(async move {
			let mut ret: Vec<Log> = Vec::new();
			if let Some(hash) = filter.block_hash.clone() {
				let id = match frontier_backend_client::load_hash::<B>(backend.as_ref(), hash)
					.map_err(|err| internal_err(format!("{:?}", err)))?
				{
					Some(hash) => hash,
					_ => return Ok(Vec::new()),
				};
				let substrate_hash = client
					.expect_block_hash_from_id(&id)
					.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

				let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
					client.as_ref(),
					id,
				);

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
					.clone()
					.and_then(|v| v.to_min_block_num())
					.map(|s| s.unique_saturated_into())
					.unwrap_or(best_number);

				if current_number > best_number {
					current_number = best_number;
				}

				let from_number = filter
					.from_block
					.clone()
					.and_then(|v| v.to_min_block_num())
					.map(|s| s.unique_saturated_into())
					.unwrap_or(client.info().best_number);

				let _ = filter_range_logs(
					client.as_ref(),
					backend.as_ref(),
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
		})
	}

	fn work(&self) -> Result<Work> {
		Ok(Work::default())
	}

	fn submit_work(&self, _: H64, _: H256, _: H256) -> Result<bool> {
		Ok(false)
	}

	fn submit_hashrate(&self, _: U256, _: H256) -> Result<bool> {
		Ok(false)
	}

	fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistory> {
		// The max supported range size is 1024 by spec.
		let range_limit = U256::from(1024);
		let block_count = if block_count > range_limit {
			range_limit.as_u64()
		} else {
			block_count.as_u64()
		};

		if let Ok(Some(id)) = frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(newest_block),
		) {
			let header = match self.client.header(id) {
				Ok(Some(h)) => h,
				_ => {
					return Err(internal_err(format!("Failed to retrieve header at {}", id)));
				}
			};
			let number = match self.client.number(header.hash()) {
				Ok(Some(n)) => n,
				_ => {
					return Err(internal_err(format!(
						"Failed to retrieve block number at {}",
						id
					)));
				}
			};
			// Highest and lowest block number within the requested range.
			let highest = UniqueSaturatedInto::<u64>::unique_saturated_into(number);
			let lowest = highest.saturating_sub(block_count);
			// Tip of the chain.
			let best_number =
				UniqueSaturatedInto::<u64>::unique_saturated_into(self.client.info().best_number);
			// Only support in-cache queries.
			if lowest < best_number.saturating_sub(self.fee_history_limit) {
				return Err(internal_err(format!("Block range out of bounds.")));
			}
			if let Ok(fee_history_cache) = &self.fee_history_cache.lock() {
				let mut response = FeeHistory {
					oldest_block: U256::from(lowest),
					base_fee_per_gas: Vec::new(),
					gas_used_ratio: Vec::new(),
					reward: None,
				};
				let mut rewards = Vec::new();
				// Iterate over the requested block range.
				for n in lowest..highest + 1 {
					if let Some(block) = fee_history_cache.get(&n) {
						response.base_fee_per_gas.push(U256::from(block.base_fee));
						response.gas_used_ratio.push(block.gas_used_ratio);
						// If the request includes reward percentiles, get them from the cache.
						if let Some(ref requested_percentiles) = reward_percentiles {
							let mut block_rewards = Vec::new();
							// Resolution is half a point. I.e. 1.0,1.5
							let resolution_per_percentile: f64 = 2.0;
							// Get cached reward for each provided percentile.
							for p in requested_percentiles {
								// Find the cache index from the user percentile.
								let p = p.clamp(0.0, 100.0);
								let index = ((p.round() / 2f64) * 2f64) * resolution_per_percentile;
								// Get and push the reward.
								let reward = if let Some(r) = block.rewards.get(index as usize) {
									U256::from(*r)
								} else {
									U256::zero()
								};
								block_rewards.push(reward);
							}
							// Push block rewards.
							rewards.push(block_rewards);
						}
					}
				}
				if rewards.len() > 0 {
					response.reward = Some(rewards);
				}
				// Calculate next base fee.
				if let (Some(last_gas_used), Some(last_fee_per_gas)) = (
					response.gas_used_ratio.last(),
					response.base_fee_per_gas.last(),
				) {
					let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
						self.client.as_ref(),
						id,
					);
					let handler = self
						.overrides
						.schemas
						.get(&schema)
						.unwrap_or(&self.overrides.fallback);
					let default_elasticity = sp_runtime::Permill::from_parts(125_000);
					let elasticity = handler
						.elasticity(&id)
						.unwrap_or(default_elasticity)
						.deconstruct();
					let elasticity = elasticity as f64 / 1_000_000f64;
					let last_fee_per_gas = last_fee_per_gas.as_u64() as f64;
					if last_gas_used > &0.5 {
						// Increase base gas
						let increase = ((last_gas_used - 0.5) * 2f64) * elasticity;
						let new_base_fee =
							(last_fee_per_gas + (last_fee_per_gas * increase)) as u64;
						response.base_fee_per_gas.push(U256::from(new_base_fee));
					} else if last_gas_used < &0.5 {
						// Decrease base gas
						let increase = ((0.5 - last_gas_used) * 2f64) * elasticity;
						let new_base_fee =
							(last_fee_per_gas - (last_fee_per_gas * increase)) as u64;
						response.base_fee_per_gas.push(U256::from(new_base_fee));
					} else {
						// Same base gas
						response
							.base_fee_per_gas
							.push(U256::from(last_fee_per_gas as u64));
					}
				}
				return Ok(response);
			} else {
				return Err(internal_err(format!("Failed to read fee history cache.")));
			}
		}
		Err(internal_err(format!(
			"Failed to retrieve requested block {:?}.",
			newest_block
		)))
	}
}

pub struct EthFilterApi<B: BlockT, C, BE> {
	client: Arc<C>,
	backend: Arc<fc_db::Backend<B>>,
	filter_pool: FilterPool,
	max_stored_filters: usize,
	max_past_logs: u32,
	block_data_cache: Arc<EthBlockDataCache<B>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, C, BE> EthFilterApi<B, C, BE> {
	pub fn new(
		client: Arc<C>,
		backend: Arc<fc_db::Backend<B>>,
		filter_pool: FilterPool,
		max_stored_filters: usize,
		max_past_logs: u32,
		block_data_cache: Arc<EthBlockDataCache<B>>,
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

impl<B, C, BE> EthFilterApi<B, C, BE>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: HeaderBackend<B> + Send + Sync + 'static,
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
			let last_key = match locked.iter().next_back() {
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

impl<B, C, BE> EthFilterApiT for EthFilterApi<B, C, BE>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
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

	fn filter_changes(&self, index: Index) -> BoxFuture<Result<FilterChanges>> {
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
			Error(jsonrpc_core::Error),
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
							.clone()
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
							.clone()
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
		let backend = Arc::clone(&self.backend);
		let max_past_logs = self.max_past_logs;

		Box::pin(async move {
			match path {
				FuturePath::Error(err) => Err(err),
				FuturePath::Block { last, next } => {
					let mut ethereum_hashes: Vec<H256> = Vec::new();
					for n in last..next {
						let id = BlockId::Number(n.unique_saturated_into());
						let substrate_hash =
							client.expect_block_hash_from_id(&id).map_err(|_| {
								internal_err(format!("Expect block number from id: {}", id))
							})?;

						let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
							client.as_ref(),
							id,
						);

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
						backend.as_ref(),
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
		})
	}

	fn filter_logs(&self, index: Index) -> BoxFuture<Result<Vec<Log>>> {
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
		let backend = Arc::clone(&self.backend);
		let max_past_logs = self.max_past_logs;

		Box::pin(async move {
			let filter = filter_result?;

			let best_number = client.info().best_number;
			let mut current_number = filter
				.to_block
				.clone()
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(best_number);

			if current_number > best_number {
				current_number = best_number;
			}

			if current_number > client.info().best_number {
				current_number = client.info().best_number;
			}

			let from_number = filter
				.from_block
				.clone()
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(client.info().best_number);

			let mut ret: Vec<Log> = Vec::new();
			let _ = filter_range_logs(
				client.as_ref(),
				backend.as_ref(),
				&block_data_cache,
				&mut ret,
				max_past_logs,
				&filter,
				from_number,
				current_number,
			)
			.await?;
			Ok(ret)
		})
	}

	fn uninstall_filter(&self, index: Index) -> Result<bool> {
		let key = U256::from(index.value());
		let pool = self.filter_pool.clone();
		// Try to lock.
		let response = if let Ok(locked) = &mut pool.lock() {
			if let Some(_) = locked.remove(&key) {
				Ok(true)
			} else {
				Err(internal_err(format!("Filter id {:?} does not exist.", key)))
			}
		} else {
			Err(internal_err("Filter pool is not available."))
		};
		response
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
			while let Some((hash, changes)) = stream.next().await {
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
