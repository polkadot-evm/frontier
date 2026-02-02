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

use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

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
	forced_parent_hashes: Option<BTreeMap<H256, H256>>,
	/// Something that can create the inherent data providers for pending state.
	pending_create_inherent_data_providers: CIDP,
	pending_consensus_data_provider: Option<Box<dyn pending::ConsensusDataProvider<B>>>,
	_marker: PhantomData<(BE, EC)>,
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
			forced_parent_hashes,
			pending_create_inherent_data_providers,
			pending_consensus_data_provider,
			_marker: PhantomData,
		}
	}

	pub async fn block_info_by_number(
		&self,
		number_or_hash: BlockNumberOrHash,
	) -> RpcResult<BlockInfo<B::Hash>> {
		// Derive the block number from the request.
		let block_number: Option<u64> = match number_or_hash {
			BlockNumberOrHash::Num(n) => Some(n),
			BlockNumberOrHash::Latest => {
				Some(self.client.info().best_number.unique_saturated_into())
			}
			BlockNumberOrHash::Earliest => Some(0),
			BlockNumberOrHash::Safe | BlockNumberOrHash::Finalized => {
				Some(self.client.info().finalized_number.unique_saturated_into())
			}
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
		};

		// Query mapping-sync for the ethereum block hash by block number.
		// This ensures consistency: if a block is visible, its transaction
		// receipts are also available.
		let eth_block_hash = match block_number {
			Some(n) => self
				.backend
				.block_hash_by_number(n)
				.await
				.map_err(|err| internal_err(format!("{err:?}")))?,
			None => None,
		};

		let Some(eth_hash) = eth_block_hash else {
			return Ok(BlockInfo::default());
		};

		// Get substrate hash(es) for this ethereum block hash
		let substrate_hashes = frontier_backend_client::load_hash::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			eth_hash,
		)
		.await
		.map_err(|err| internal_err(format!("{err:?}")))?;

		let Some(substrate_hash) = substrate_hashes else {
			return Ok(BlockInfo::default());
		};

		// Verify the substrate hash is on the canonical chain for all non-genesis blocks.
		// The mapping is written at block import time, not finalization. If mapping-sync
		// is lagging or processed an orphan block, the mapping could be stale even for
		// finalized block numbers. We always verify against the canonical chain to ensure
		// consistency, with genesis (block 0) as the only exception since it's immutable.
		if let Some(block_num) = block_number {
			if block_num > 0 {
				let canonical_hash = self
					.client
					.hash(block_num.unique_saturated_into())
					.map_err(|e| internal_err(format!("{e:?}")))?;

				if canonical_hash != Some(substrate_hash) {
					// Mapping is stale - treat as not indexed yet
					return Ok(BlockInfo::default());
				}
			}
		}

		self.block_info_by_substrate_hash(substrate_hash).await
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
			forced_parent_hashes,
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
			forced_parent_hashes,
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

	fn author(&self) -> RpcResult<H160> {
		self.author()
	}

	fn accounts(&self) -> RpcResult<Vec<H160>> {
		self.accounts()
	}

	fn block_number(&self) -> RpcResult<U256> {
		self.block_number()
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
