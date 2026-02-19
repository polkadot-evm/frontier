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

mod block;
mod client;
mod execute;
mod fee;
pub(crate) mod filter;
pub mod format;
mod mining;
pub mod pending;
mod state;
mod submit;
mod transaction;

use std::{
	collections::BTreeMap,
	marker::PhantomData,
	sync::{Arc, Mutex},
};

use ethereum::{BlockV3 as EthereumBlock, TransactionV3 as EthereumTransaction};
use ethereum_types::{H160, H256, H64, U256, U64};
use jsonrpsee::core::{async_trait, RpcResult};
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_network_sync::SyncingService;
use sc_transaction_pool_api::TransactionPool;
use sp_api::{CallApiAt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};
// Frontier
use fc_rpc_core::{types::*, EthApiServer};
use fc_storage::StorageOverride;
use fp_rpc::{
	ConvertTransaction, ConvertTransactionRuntimeApi, EthereumRuntimeRPCApi,
	RuntimeStorageOverride, TransactionStatus,
};

use crate::{
	cache::EthBlockDataCacheTask, frontier_backend_client, internal_err, public_key,
	signer::EthSigner,
};

pub use self::{execute::EstimateGasAdapter, filter::EthFilter};

// Configuration trait for RPC configuration.
pub trait EthConfig<B: BlockT, C>: Send + Sync + 'static {
	type EstimateGasAdapter: EstimateGasAdapter + Send + Sync;
	type RuntimeStorageOverride: RuntimeStorageOverride<B, C>;
}

impl<B: BlockT, C> EthConfig<B, C> for () {
	type EstimateGasAdapter = ();
	type RuntimeStorageOverride = ();
}

const LATEST_READABLE_SCAN_LIMIT: u64 = 128;

/// Eth API implementation.
pub struct Eth<B: BlockT, C, P, CT, BE, CIDP, EC> {
	pool: Arc<P>,
	client: Arc<C>,
	convert_transaction: Option<CT>,
	sync: Arc<SyncingService<B>>,
	is_authority: bool,
	signers: Vec<Box<dyn EthSigner>>,
	storage_override: Arc<dyn StorageOverride<B>>,
	backend: Arc<dyn fc_api::Backend<B>>,
	block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	fee_history_cache: FeeHistoryCache,
	fee_history_cache_limit: FeeHistoryCacheLimit,
	/// When using eth_call/eth_estimateGas, the maximum allowed gas limit will be
	/// block.gas_limit * execute_gas_limit_multiplier
	execute_gas_limit_multiplier: u64,
	/// Whether RPC submission accepts legacy transactions without EIP-155 chain id.
	rpc_allow_unprotected_txs: bool,
	forced_parent_hashes: Option<BTreeMap<H256, H256>>,
	latest_readable_scan_limit: u64,
	last_readable_latest: Mutex<Option<B::Hash>>,
	/// Something that can create the inherent data providers for pending state.
	pending_create_inherent_data_providers: CIDP,
	pending_consensus_data_provider: Option<Box<dyn pending::ConsensusDataProvider<B>>>,
	_marker: PhantomData<(BE, EC)>,
}

fn find_readable_hash_from_number_desc<H, FReadable, FHashAt>(
	start_number: u64,
	stop_number: Option<u64>,
	is_readable: &mut FReadable,
	hash_at_number: &mut FHashAt,
) -> (Option<H>, u64)
where
	FReadable: FnMut(&H) -> bool,
	FHashAt: FnMut(u64) -> Option<H>,
{
	let lower_bound = stop_number.unwrap_or(0);
	if start_number < lower_bound {
		return (None, 0);
	}

	let mut current_number = start_number;
	let mut scanned_hops: u64 = 0;

	loop {
		let Some(hash) = hash_at_number(current_number) else {
			break;
		};
		if is_readable(&hash) {
			return (Some(hash), scanned_hops);
		}
		if current_number == lower_bound || current_number == 0 {
			break;
		}
		current_number = current_number.saturating_sub(1);
		scanned_hops = scanned_hops.saturating_add(1);
	}

	(None, scanned_hops)
}

async fn resolve_canonical_substrate_hash_by_number<B, C>(
	client: &C,
	backend: &dyn fc_api::Backend<B>,
	block_number: u64,
) -> RpcResult<Option<B::Hash>>
where
	B: BlockT,
	C: HeaderBackend<B> + 'static,
{
	let canonical_hash = client
		.hash(block_number.unique_saturated_into())
		.map_err(|e| internal_err(format!("{e:?}")))?;
	let Some(canonical_hash) = canonical_hash else {
		return Ok(None);
	};

	if let Some(eth_hash) = backend
		.block_hash_by_number(block_number)
		.await
		.map_err(|err| internal_err(format!("{err:?}")))?
	{
		let substrate_hash = frontier_backend_client::load_hash::<B, C>(client, backend, eth_hash)
			.await
			.map_err(|err| internal_err(format!("{err:?}")))?;
		if substrate_hash == Some(canonical_hash) {
			return Ok(Some(canonical_hash));
		}
	}

	Ok(Some(canonical_hash))
}

impl<B, C, P, CT, BE, CIDP, EC> Eth<B, C, P, CT, BE, CIDP, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
{
	pub fn new(
		client: Arc<C>,
		pool: Arc<P>,
		convert_transaction: Option<CT>,
		sync: Arc<SyncingService<B>>,
		signers: Vec<Box<dyn EthSigner>>,
		storage_override: Arc<dyn StorageOverride<B>>,
		backend: Arc<dyn fc_api::Backend<B>>,
		is_authority: bool,
		block_data_cache: Arc<EthBlockDataCacheTask<B>>,
		fee_history_cache: FeeHistoryCache,
		fee_history_cache_limit: FeeHistoryCacheLimit,
		execute_gas_limit_multiplier: u64,
		rpc_allow_unprotected_txs: bool,
		forced_parent_hashes: Option<BTreeMap<H256, H256>>,
		pending_create_inherent_data_providers: CIDP,
		pending_consensus_data_provider: Option<Box<dyn pending::ConsensusDataProvider<B>>>,
	) -> Self {
		Self {
			client,
			pool,
			convert_transaction,
			sync,
			is_authority,
			signers,
			storage_override,
			backend,
			block_data_cache,
			fee_history_cache,
			fee_history_cache_limit,
			execute_gas_limit_multiplier,
			rpc_allow_unprotected_txs,
			forced_parent_hashes,
			latest_readable_scan_limit: LATEST_READABLE_SCAN_LIMIT,
			last_readable_latest: Mutex::new(None),
			pending_create_inherent_data_providers,
			pending_consensus_data_provider,
			_marker: PhantomData,
		}
	}

	fn cached_latest_hash_is_usable(
		&self,
		cached_hash: &B::Hash,
		latest_indexed_number: u64,
	) -> RpcResult<bool> {
		let Some(cached_number) = self
			.client
			.number(*cached_hash)
			.map_err(|err| internal_err(format!("{err:?}")))?
		else {
			return Ok(false);
		};
		let cached_number: u64 = cached_number.unique_saturated_into();
		if cached_number > latest_indexed_number {
			return Ok(false);
		}

		let canonical_hash = self
			.client
			.hash(cached_number.unique_saturated_into())
			.map_err(|err| internal_err(format!("{err:?}")))?;
		if canonical_hash != Some(*cached_hash) {
			return Ok(false);
		}

		Ok(self.storage_override.current_block(*cached_hash).is_some())
	}

	async fn latest_indexed_hash_with_block(&self) -> RpcResult<B::Hash> {
		let latest_indexed_hash = self
			.backend
			.latest_block_hash()
			.await
			.map_err(|err| internal_err(format!("{err:?}")))?;
		let latest_indexed_number: u64 = self
			.client
			.number(latest_indexed_hash)
			.map_err(|err| internal_err(format!("{err:?}")))?
			.ok_or_else(|| internal_err("Block number not found for latest indexed block"))?
			.unique_saturated_into();

		let cached_hash = *self
			.last_readable_latest
			.lock()
			.map_err(|_| internal_err("last_readable_latest lock poisoned"))?;
		if let Some(cached_hash) = cached_hash {
			if self.cached_latest_hash_is_usable(&cached_hash, latest_indexed_number)? {
				log::debug!(
					target: "rpc",
					"latest readable selection cache_hit=true bounded_hit=false exhaustive_hit=false full_miss=false bounded_scanned_hops=0 exhaustive_scanned_hops=0 limit={}",
					self.latest_readable_scan_limit,
				);
				return Ok(cached_hash);
			}
		}

		let bounded_lower = latest_indexed_number.saturating_sub(self.latest_readable_scan_limit);
		let (bounded_resolved_hash, bounded_scanned_hops) = find_readable_hash_from_number_desc(
			latest_indexed_number,
			Some(bounded_lower),
			&mut |hash: &B::Hash| self.storage_override.current_block(*hash).is_some(),
			&mut |number: u64| {
				self.client
					.hash(number.unique_saturated_into())
					.map_err(|err| internal_err(format!("{err:?}")))
					.ok()
					.flatten()
			},
		);

		let (selected_hash, bounded_hit, exhaustive_hit, full_miss, exhaustive_scanned_hops) =
			if let Some(resolved_hash) = bounded_resolved_hash {
				(resolved_hash, true, false, false, 0)
			} else {
				let exhaustive_start = bounded_lower.checked_sub(1);
				let (exhaustive_resolved_hash, exhaustive_scanned_hops) =
					if let Some(exhaustive_start) = exhaustive_start {
						find_readable_hash_from_number_desc(
							exhaustive_start,
							Some(0),
							&mut |hash: &B::Hash| {
								self.storage_override.current_block(*hash).is_some()
							},
							&mut |number: u64| {
								self.client
									.hash(number.unique_saturated_into())
									.map_err(|err| internal_err(format!("{err:?}")))
									.ok()
									.flatten()
							},
						)
					} else {
						(None, 0)
					};

				if let Some(resolved_hash) = exhaustive_resolved_hash {
					(resolved_hash, false, true, false, exhaustive_scanned_hops)
				} else {
					(
						latest_indexed_hash,
						false,
						false,
						true,
						exhaustive_scanned_hops,
					)
				}
			};

		if !full_miss {
			self.last_readable_latest
				.lock()
				.map_err(|_| internal_err("last_readable_latest lock poisoned"))?
				.replace(selected_hash);
		}

		log::debug!(
			target: "rpc",
			"latest readable selection cache_hit=false bounded_hit={} exhaustive_hit={} full_miss={} bounded_scanned_hops={} exhaustive_scanned_hops={} limit={}",
			bounded_hit,
			exhaustive_hit,
			full_miss,
			bounded_scanned_hops,
			exhaustive_scanned_hops,
			self.latest_readable_scan_limit,
		);

		Ok(selected_hash)
	}

	pub async fn block_info_by_number(
		&self,
		number_or_hash: BlockNumberOrHash,
	) -> RpcResult<BlockInfo<B::Hash>> {
		// Handle special cases that don't use block number lookup
		match number_or_hash {
			BlockNumberOrHash::Pending => {
				// Pending blocks are not indexed in mapping-sync.
				// Return empty BlockInfo - pending blocks are handled specially
				// by methods that need them (e.g., pending_block()).
				return Ok(BlockInfo::default());
			}
			BlockNumberOrHash::Hash { hash, .. } => {
				// For hash queries, use the existing eth block hash lookup
				return self.block_info_by_eth_block_hash(hash).await;
			}
			BlockNumberOrHash::Latest => {
				// For "latest", use the latest indexed block and fall back to the nearest
				// canonical ancestor that has a readable block payload.
				let substrate_hash = self.latest_indexed_hash_with_block().await?;
				return self.block_info_by_substrate_hash(substrate_hash).await;
			}
			_ => {}
		}

		// Derive the block number from the request.
		let block_number: u64 = match number_or_hash {
			BlockNumberOrHash::Num(n) => n,
			BlockNumberOrHash::Earliest => 0,
			BlockNumberOrHash::Safe | BlockNumberOrHash::Finalized => {
				self.client.info().finalized_number.unique_saturated_into()
			}
			// Already handled above
			BlockNumberOrHash::Latest
			| BlockNumberOrHash::Pending
			| BlockNumberOrHash::Hash { .. } => unreachable!(),
		};

		let Some(canonical_hash) = resolve_canonical_substrate_hash_by_number::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			block_number,
		)
		.await?
		else {
			return Ok(BlockInfo::default());
		};

		self.block_info_by_substrate_hash(canonical_hash).await
	}

	pub async fn block_info_by_eth_block_hash(
		&self,
		eth_block_hash: H256,
	) -> RpcResult<BlockInfo<B::Hash>> {
		let substrate_hash = match frontier_backend_client::load_hash::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			eth_block_hash,
		)
		.await
		.map_err(|err| internal_err(format!("{err:?}")))?
		{
			Some(hash) => hash,
			_ => return Ok(BlockInfo::default()),
		};

		self.block_info_by_substrate_hash(substrate_hash).await
	}

	pub async fn block_info_by_eth_transaction_hash(
		&self,
		ethereum_tx_hash: H256,
	) -> RpcResult<(BlockInfo<B::Hash>, usize)> {
		let (eth_block_hash, index) = match frontier_backend_client::load_transactions::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			ethereum_tx_hash,
			true,
		)
		.await
		.map_err(|err| internal_err(format!("{err:?}")))?
		{
			Some((hash, index)) => (hash, index as usize),
			None => return Ok((BlockInfo::default(), 0)),
		};

		let substrate_hash = match frontier_backend_client::load_hash::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			eth_block_hash,
		)
		.await
		.map_err(|err| internal_err(format!("{err:?}")))?
		{
			Some(hash) => hash,
			_ => return Ok((BlockInfo::default(), 0)),
		};

		Ok((
			self.block_info_by_substrate_hash(substrate_hash).await?,
			index,
		))
	}

	pub async fn block_info_by_substrate_hash(
		&self,
		substrate_hash: B::Hash,
	) -> RpcResult<BlockInfo<B::Hash>> {
		let block = self.block_data_cache.current_block(substrate_hash).await;
		let statuses = self
			.block_data_cache
			.current_transaction_statuses(substrate_hash)
			.await;
		let receipts = self.storage_override.current_receipts(substrate_hash);
		let is_eip1559 = self.storage_override.is_eip1559(substrate_hash);
		let base_fee = self
			.client
			.runtime_api()
			.gas_price(substrate_hash)
			.unwrap_or_default();

		Ok(BlockInfo::new(
			block,
			receipts,
			statuses,
			substrate_hash,
			is_eip1559,
			base_fee,
		))
	}
}

impl<B, C, P, CT, BE, CIDP, EC> Eth<B, C, P, CT, BE, CIDP, EC>
where
	B: BlockT,
	EC: EthConfig<B, C>,
{
	pub fn replace_config<EC2: EthConfig<B, C>>(self) -> Eth<B, C, P, CT, BE, CIDP, EC2> {
		let Self {
			client,
			pool,
			convert_transaction,
			sync,
			is_authority,
			signers,
			storage_override,
			backend,
			block_data_cache,
			fee_history_cache,
			fee_history_cache_limit,
			execute_gas_limit_multiplier,
			rpc_allow_unprotected_txs,
			forced_parent_hashes,
			latest_readable_scan_limit,
			last_readable_latest,
			pending_create_inherent_data_providers,
			pending_consensus_data_provider,
			_marker: _,
		} = self;

		Eth {
			client,
			pool,
			convert_transaction,
			sync,
			is_authority,
			signers,
			storage_override,
			backend,
			block_data_cache,
			fee_history_cache,
			fee_history_cache_limit,
			execute_gas_limit_multiplier,
			rpc_allow_unprotected_txs,
			forced_parent_hashes,
			latest_readable_scan_limit,
			last_readable_latest,
			pending_create_inherent_data_providers,
			pending_consensus_data_provider,
			_marker: PhantomData,
		}
	}
}

#[async_trait]
impl<B, C, P, CT, BE, CIDP, EC> EthApiServer for Eth<B, C, P, CT, BE, CIDP, EC>
where
	B: BlockT,
	C: CallApiAt<B> + ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + ConvertTransactionRuntimeApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	P: TransactionPool<Block = B, Hash = B::Hash> + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
	CIDP: CreateInherentDataProviders<B, ()> + Send + 'static,
	EC: EthConfig<B, C>,
{
	// ########################################################################
	// Client
	// ########################################################################

	fn protocol_version(&self) -> RpcResult<u64> {
		self.protocol_version()
	}

	async fn syncing(&self) -> RpcResult<SyncStatus> {
		self.syncing().await
	}

	async fn author(&self) -> RpcResult<H160> {
		self.author().await
	}

	fn accounts(&self) -> RpcResult<Vec<H160>> {
		self.accounts()
	}

	async fn block_number(&self) -> RpcResult<U256> {
		self.block_number().await
	}

	fn chain_id(&self) -> RpcResult<Option<U64>> {
		self.chain_id()
	}

	// ########################################################################
	// Block
	// ########################################################################

	async fn block_by_hash(&self, hash: H256, full: bool) -> RpcResult<Option<RichBlock>> {
		self.block_by_hash(hash, full).await
	}

	async fn block_by_number(
		&self,
		number_or_hash: BlockNumberOrHash,
		full: bool,
	) -> RpcResult<Option<RichBlock>> {
		self.block_by_number(number_or_hash, full).await
	}

	async fn block_transaction_count_by_hash(&self, hash: H256) -> RpcResult<Option<U256>> {
		self.block_transaction_count_by_hash(hash).await
	}

	async fn block_transaction_count_by_number(
		&self,
		number_or_hash: BlockNumberOrHash,
	) -> RpcResult<Option<U256>> {
		self.block_transaction_count_by_number(number_or_hash).await
	}

	async fn block_transaction_receipts(
		&self,
		number_or_hash: BlockNumberOrHash,
	) -> RpcResult<Option<Vec<Receipt>>> {
		self.block_transaction_receipts(number_or_hash).await
	}

	fn block_uncles_count_by_hash(&self, hash: H256) -> RpcResult<U256> {
		self.block_uncles_count_by_hash(hash)
	}

	fn block_uncles_count_by_number(&self, number_or_hash: BlockNumberOrHash) -> RpcResult<U256> {
		self.block_uncles_count_by_number(number_or_hash)
	}

	fn uncle_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> RpcResult<Option<RichBlock>> {
		self.uncle_by_block_hash_and_index(hash, index)
	}

	fn uncle_by_block_number_and_index(
		&self,
		number_or_hash: BlockNumberOrHash,
		index: Index,
	) -> RpcResult<Option<RichBlock>> {
		self.uncle_by_block_number_and_index(number_or_hash, index)
	}

	// ########################################################################
	// Transaction
	// ########################################################################

	async fn transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>> {
		self.transaction_by_hash(hash).await
	}

	async fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> RpcResult<Option<Transaction>> {
		self.transaction_by_block_hash_and_index(hash, index).await
	}

	async fn transaction_by_block_number_and_index(
		&self,
		number_or_hash: BlockNumberOrHash,
		index: Index,
	) -> RpcResult<Option<Transaction>> {
		self.transaction_by_block_number_and_index(number_or_hash, index)
			.await
	}

	async fn transaction_receipt(&self, hash: H256) -> RpcResult<Option<Receipt>> {
		let (block_info, index) = self.block_info_by_eth_transaction_hash(hash).await?;
		self.transaction_receipt(&block_info, hash, index).await
	}

	// ########################################################################
	// State
	// ########################################################################

	async fn balance(
		&self,
		address: H160,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<U256> {
		self.balance(address, number_or_hash).await
	}

	async fn storage_at(
		&self,
		address: H160,
		index: U256,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<H256> {
		self.storage_at(address, index, number_or_hash).await
	}

	async fn transaction_count(
		&self,
		address: H160,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<U256> {
		self.transaction_count(address, number_or_hash).await
	}

	async fn pending_transactions(&self) -> RpcResult<Vec<Transaction>> {
		self.pending_transactions().await
	}

	async fn code_at(
		&self,
		address: H160,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<Bytes> {
		self.code_at(address, number_or_hash).await
	}

	// ########################################################################
	// Execute
	// ########################################################################

	async fn call(
		&self,
		request: TransactionRequest,
		number_or_hash: Option<BlockNumberOrHash>,
		state_overrides: Option<BTreeMap<H160, CallStateOverride>>,
	) -> RpcResult<Bytes> {
		self.call(request, number_or_hash, state_overrides).await
	}

	async fn estimate_gas(
		&self,
		request: TransactionRequest,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<U256> {
		self.estimate_gas(request, number_or_hash).await
	}

	// ########################################################################
	// Fee
	// ########################################################################

	fn gas_price(&self) -> RpcResult<U256> {
		self.gas_price()
	}

	async fn fee_history(
		&self,
		block_count: BlockCount,
		newest_block: BlockNumberOrHash,
		reward_percentiles: Option<Vec<f64>>,
	) -> RpcResult<FeeHistory> {
		self.fee_history(block_count.into(), newest_block, reward_percentiles)
			.await
	}

	fn max_priority_fee_per_gas(&self) -> RpcResult<U256> {
		self.max_priority_fee_per_gas()
	}

	// ########################################################################
	// Mining
	// ########################################################################

	fn is_mining(&self) -> RpcResult<bool> {
		self.is_mining()
	}

	fn hashrate(&self) -> RpcResult<U256> {
		self.hashrate()
	}

	fn work(&self) -> RpcResult<Work> {
		self.work()
	}

	fn submit_hashrate(&self, hashrate: U256, id: H256) -> RpcResult<bool> {
		self.submit_hashrate(hashrate, id)
	}

	fn submit_work(&self, nonce: H64, pow_hash: H256, mix_digest: H256) -> RpcResult<bool> {
		self.submit_work(nonce, pow_hash, mix_digest)
	}

	// ########################################################################
	// Submit
	// ########################################################################

	async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<H256> {
		self.send_transaction(request).await
	}

	async fn send_raw_transaction(&self, bytes: Bytes) -> RpcResult<H256> {
		self.send_raw_transaction(bytes).await
	}
}

fn rich_block_build(
	block: EthereumBlock,
	statuses: Vec<Option<TransactionStatus>>,
	hash: Option<H256>,
	full_transactions: bool,
	base_fee: Option<U256>,
	is_pending: bool,
) -> RichBlock {
	let (hash, miner, nonce, total_difficulty) = if !is_pending {
		(
			Some(hash.unwrap_or_else(|| H256::from(keccak_256(&rlp::encode(&block.header))))),
			Some(block.header.beneficiary),
			Some(block.header.nonce),
			Some(U256::zero()),
		)
	} else {
		(None, None, None, None)
	};
	Rich {
		inner: Block {
			header: Header {
				hash,
				parent_hash: block.header.parent_hash,
				uncles_hash: block.header.ommers_hash,
				author: block.header.beneficiary,
				miner,
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
				nonce,
				size: Some(U256::from(rlp::encode(&block.header).len() as u32)),
			},
			total_difficulty,
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
									transaction,
									Some(&block),
									statuses[index].as_ref(),
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
	ethereum_transaction: &EthereumTransaction,
	block: Option<&EthereumBlock>,
	status: Option<&TransactionStatus>,
	base_fee: Option<U256>,
) -> Transaction {
	let pubkey = public_key(ethereum_transaction).ok();
	let from = status.map_or(
		{
			match pubkey {
				Some(pk) => H160::from(H256::from(keccak_256(&pk))),
				_ => H160::default(),
			}
		},
		|status| status.from,
	);

	let mut transaction: Transaction = Transaction::build_from(from, ethereum_transaction);

	if let EthereumTransaction::EIP1559(_) = ethereum_transaction {
		if block.is_none() && status.is_none() {
			// If transaction is not mined yet, gas price is considered just max fee per gas.
		} else {
			let base_fee = base_fee.unwrap_or_default();
			let max_priority_fee_per_gas = transaction.max_priority_fee_per_gas.unwrap_or_default();
			let max_fee_per_gas = transaction.max_fee_per_gas.unwrap_or_default();
			// If transaction is already mined, gas price is the effective gas price.
			transaction.gas_price = Some(
				base_fee
					.checked_add(max_priority_fee_per_gas)
					.unwrap_or_else(U256::max_value)
					.min(max_fee_per_gas),
			);
		}
	}

	// Block hash.
	transaction.block_hash = block.map(|block| block.header.hash());
	// Block number.
	transaction.block_number = block.map(|block| block.header.number);
	// Transaction index.
	transaction.transaction_index = status.map(|status| {
		U256::from(UniqueSaturatedInto::<u32>::unique_saturated_into(
			status.transaction_index,
		))
	});
	// Creates.
	transaction.creates = status.and_then(|status| status.contract_address);

	transaction
}

/// The most commonly used block information in the rpc interfaces.
#[derive(Clone, Default)]
pub struct BlockInfo<H> {
	block: Option<EthereumBlock>,
	receipts: Option<Vec<ethereum::ReceiptV4>>,
	statuses: Option<Vec<TransactionStatus>>,
	substrate_hash: H,
	is_eip1559: bool,
	base_fee: U256,
}

impl<H> BlockInfo<H> {
	pub fn new(
		block: Option<EthereumBlock>,
		receipts: Option<Vec<ethereum::ReceiptV4>>,
		statuses: Option<Vec<TransactionStatus>>,
		substrate_hash: H,
		is_eip1559: bool,
		base_fee: U256,
	) -> Self {
		Self {
			block,
			receipts,
			statuses,
			substrate_hash,
			is_eip1559,
			base_fee,
		}
	}
}

#[cfg(test)]
fn test_only_select_latest_readable_hash(
	latest_hash: u64,
	latest_number: u64,
	scan_limit: u64,
	cached_hash: Option<u64>,
	readable_at_or_below: Option<u64>,
	cached_usable: bool,
) -> (u64, Option<u64>, u64, u64) {
	if let Some(cached_hash) = cached_hash {
		if cached_usable {
			return (cached_hash, Some(cached_hash), 0, 0);
		}
	}

	let bounded_lower = latest_number.saturating_sub(scan_limit);
	let (bounded_resolved, bounded_hops) = find_readable_hash_from_number_desc(
		latest_number,
		Some(bounded_lower),
		&mut |hash: &u64| readable_at_or_below.is_some_and(|limit| *hash <= limit),
		&mut |number: u64| Some(number),
	);

	if let Some(resolved) = bounded_resolved {
		return (resolved, Some(resolved), bounded_hops, 0);
	}

	let (exhaustive_resolved, exhaustive_hops) = if bounded_lower == 0 {
		(None, 0)
	} else {
		find_readable_hash_from_number_desc(
			bounded_lower.saturating_sub(1),
			Some(0),
			&mut |hash: &u64| readable_at_or_below.is_some_and(|limit| *hash <= limit),
			&mut |number: u64| Some(number),
		)
	};

	if let Some(resolved) = exhaustive_resolved {
		return (resolved, Some(resolved), bounded_hops, exhaustive_hops);
	}

	(latest_hash, None, bounded_hops, exhaustive_hops)
}

#[cfg(test)]
mod tests {
	use std::{path::PathBuf, sync::Arc};

	use ethereum::PartialHeader;
	use ethereum_types::{Bloom, H160, H256, H64, U256};
	use sc_block_builder::BlockBuilderBuilder;
	use sp_consensus::BlockOrigin;
	use sp_runtime::{
		generic::{Block, Header},
		traits::{BlakeTwo256, Block as BlockT},
	};
	use substrate_test_runtime_client::{
		prelude::*, DefaultTestClientBuilderExt, TestClientBuilder,
	};
	use tempfile::tempdir;

	use super::{
		resolve_canonical_substrate_hash_by_number, test_only_select_latest_readable_hash,
	};

	type OpaqueBlock =
		Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

	fn open_frontier_backend<Block: BlockT, C: sp_blockchain::HeaderBackend<Block>>(
		client: Arc<C>,
		path: PathBuf,
	) -> Arc<fc_db::kv::Backend<Block, C>> {
		Arc::new(
			fc_db::kv::Backend::<Block, C>::new(
				client,
				&fc_db::kv::DatabaseSettings {
					#[cfg(feature = "rocksdb")]
					source: sc_client_db::DatabaseSource::RocksDb {
						path,
						cache_size: 0,
					},
					#[cfg(not(feature = "rocksdb"))]
					source: sc_client_db::DatabaseSource::ParityDb { path },
				},
			)
			.expect("frontier backend"),
		)
	}

	fn make_ethereum_block(seed: u64) -> ethereum::BlockV3 {
		let partial_header = PartialHeader {
			parent_hash: H256::from_low_u64_be(seed),
			beneficiary: H160::from_low_u64_be(seed),
			state_root: H256::from_low_u64_be(seed.saturating_add(1)),
			receipts_root: H256::from_low_u64_be(seed.saturating_add(2)),
			logs_bloom: Bloom::default(),
			difficulty: U256::from(seed),
			number: U256::from(seed),
			gas_limit: U256::from(seed.saturating_add(100)),
			gas_used: U256::from(seed.saturating_add(50)),
			timestamp: seed,
			extra_data: Vec::new(),
			mix_hash: H256::from_low_u64_be(seed.saturating_add(3)),
			nonce: H64::from_low_u64_be(seed),
		};
		ethereum::Block::new(partial_header, vec![], vec![])
	}

	#[test]
	fn resolve_canonical_substrate_hash_by_number_is_read_only() {
		let tmp = tempdir().expect("create temp dir");
		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);
		let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.keep());

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
		let canonical_hash = block.header.hash();
		futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("import block");

		let ethereum_block = make_ethereum_block(1);
		let canonical_eth_hash = ethereum_block.header.hash();
		let commitment = fc_db::kv::MappingCommitment::<OpaqueBlock> {
			block_hash: canonical_hash,
			ethereum_block_hash: canonical_eth_hash,
			ethereum_transaction_hashes: vec![],
		};
		backend
			.mapping()
			.write_hashes(commitment, 1, fc_db::kv::NumberMappingWrite::Skip)
			.expect("seed hash mapping only");
		assert_eq!(
			backend
				.mapping()
				.block_hash_by_number(1)
				.expect("read number mapping"),
			None
		);
		assert_eq!(
			backend
				.mapping()
				.block_hash(&canonical_eth_hash)
				.expect("read hash mapping"),
			Some(vec![canonical_hash])
		);

		let resolved = futures::executor::block_on(resolve_canonical_substrate_hash_by_number::<
			OpaqueBlock,
			_,
		>(client.as_ref(), backend.as_ref(), 1))
		.expect("resolve missing mapping without repair");
		assert_eq!(resolved, Some(canonical_hash));
		assert_eq!(
			backend
				.mapping()
				.block_hash_by_number(1)
				.expect("read unchanged number mapping"),
			None
		);

		let stale_hash = H256::repeat_byte(0x42);
		backend
			.mapping()
			.set_block_hash_by_number(1, stale_hash)
			.expect("seed stale number mapping");
		assert_eq!(
			backend
				.mapping()
				.block_hash_by_number(1)
				.expect("read stale number mapping"),
			Some(stale_hash)
		);

		let resolved = futures::executor::block_on(resolve_canonical_substrate_hash_by_number::<
			OpaqueBlock,
			_,
		>(client.as_ref(), backend.as_ref(), 1))
		.expect("resolve stale mapping without repair");
		assert_eq!(resolved, Some(canonical_hash));
		assert_eq!(
			backend
				.mapping()
				.block_hash_by_number(1)
				.expect("read stale number mapping"),
			Some(stale_hash)
		);
	}

	#[test]
	fn latest_readable_selection_uses_exhaustive_fallback_when_bounded_scan_misses() {
		let (resolved, cached, bounded_hops, exhaustive_hops) =
			test_only_select_latest_readable_hash(100, 100, 2, None, Some(80), false);
		assert_eq!(resolved, 80);
		assert_eq!(cached, Some(80));
		assert_eq!(bounded_hops, 2);
		assert_eq!(exhaustive_hops, 17);
	}

	#[test]
	fn latest_readable_selection_uses_cache_before_scanning() {
		let (resolved, cached, bounded_hops, exhaustive_hops) =
			test_only_select_latest_readable_hash(100, 100, 2, Some(80), Some(50), true);
		assert_eq!(resolved, 80);
		assert_eq!(cached, Some(80));
		assert_eq!(bounded_hops, 0);
		assert_eq!(exhaustive_hops, 0);
	}

	#[test]
	fn latest_readable_selection_falls_back_to_latest_when_no_readable_exists() {
		let (resolved, cached, bounded_hops, exhaustive_hops) =
			test_only_select_latest_readable_hash(100, 100, 2, Some(80), None, false);
		assert_eq!(resolved, 100);
		assert_eq!(cached, None);
		assert_eq!(bounded_hops, 2);
		assert_eq!(exhaustive_hops, 97);
	}
}
