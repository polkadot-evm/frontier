// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
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

mod block;
mod cache;
mod client;
mod execute;
mod fee;
mod filter;
mod mining;
mod state;
mod submit;
mod transaction;

use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use ethereum::{BlockV2 as EthereumBlock, TransactionV2 as EthereumTransaction};
use ethereum_types::{H160, H256, H512, H64, U256, U64};
use futures::future::BoxFuture;
use jsonrpc_core::Result;

use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sc_network::{ExHashT, NetworkService};
use sc_transaction_pool::{ChainApi, Pool};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::{Core, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT, UniqueSaturatedInto},
};

use fc_rpc_core::{types::*, EthApi as EthApiT};
use fp_rpc::{ConvertTransactionRuntimeApi, EthereumRuntimeRPCApi, TransactionStatus};

use crate::{internal_err, overrides::OverrideHandle, public_key, signer::EthSigner};

pub use self::{
	cache::{EthBlockDataCache, EthTask},
	filter::EthFilterApi,
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
			block_data_cache,
			fee_history_limit,
			fee_history_cache,
			_marker: PhantomData,
		}
	}
}

impl<B, C, P, CT, BE, H: ExHashT, A> EthApiT for EthApi<B, C, P, CT, BE, H, A>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: BlockBuilderApi<B> + ConvertTransactionRuntimeApi<B> + EthereumRuntimeRPCApi<B>,
	P: TransactionPool<Block = B> + Send + Sync + 'static,
	CT: fp_rpc::ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	A: ChainApi<Block = B> + 'static,
{
	// ########################################################################
	// Client
	// ########################################################################
	fn protocol_version(&self) -> Result<u64> {
		self.protocol_version()
	}

	fn syncing(&self) -> Result<SyncStatus> {
		self.syncing()
	}

	fn author(&self) -> Result<H160> {
		self.author()
	}

	fn accounts(&self) -> Result<Vec<H160>> {
		self.accounts()
	}

	fn block_number(&self) -> Result<U256> {
		self.block_number()
	}

	fn chain_id(&self) -> Result<Option<U64>> {
		self.chain_id()
	}

	// ########################################################################
	// Block
	// ########################################################################

	fn block_by_hash(
		&self,
		hash: H256,
		full: bool,
	) -> BoxFuture<'static, Result<Option<RichBlock>>> {
		self.block_by_hash(hash, full)
	}

	fn block_by_number(
		&self,
		number: BlockNumber,
		full: bool,
	) -> BoxFuture<'static, Result<Option<RichBlock>>> {
		self.block_by_number(number, full)
	}

	fn block_transaction_count_by_hash(&self, hash: H256) -> Result<Option<U256>> {
		self.block_transaction_count_by_hash(hash)
	}

	fn block_transaction_count_by_number(&self, number: BlockNumber) -> Result<Option<U256>> {
		self.block_transaction_count_by_number(number)
	}

	fn block_uncles_count_by_hash(&self, hash: H256) -> Result<U256> {
		self.block_uncles_count_by_hash(hash)
	}

	fn block_uncles_count_by_number(&self, number: BlockNumber) -> Result<U256> {
		self.block_uncles_count_by_number(number)
	}

	fn uncle_by_block_hash_and_index(&self, hash: H256, index: Index) -> Result<Option<RichBlock>> {
		self.uncle_by_block_hash_and_index(hash, index)
	}

	fn uncle_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> Result<Option<RichBlock>> {
		self.uncle_by_block_number_and_index(number, index)
	}

	// ########################################################################
	// Transaction
	// ########################################################################

	fn transaction_by_hash(&self, hash: H256) -> BoxFuture<'static, Result<Option<Transaction>>> {
		self.transaction_by_hash(hash)
	}

	fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> BoxFuture<'static, Result<Option<Transaction>>> {
		self.transaction_by_block_hash_and_index(hash, index)
	}

	fn transaction_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> BoxFuture<'static, Result<Option<Transaction>>> {
		self.transaction_by_block_number_and_index(number, index)
	}

	fn transaction_receipt(&self, hash: H256) -> BoxFuture<'static, Result<Option<Receipt>>> {
		self.transaction_receipt(hash)
	}

	// ########################################################################
	// State
	// ########################################################################

	fn balance(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		self.balance(address, number)
	}

	fn storage_at(&self, address: H160, index: U256, number: Option<BlockNumber>) -> Result<H256> {
		self.storage_at(address, index, number)
	}

	fn transaction_count(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		self.transaction_count(address, number)
	}

	fn code_at(&self, address: H160, number: Option<BlockNumber>) -> Result<Bytes> {
		self.code_at(address, number)
	}

	// ########################################################################
	// Execute
	// ########################################################################

	fn call(&self, request: CallRequest, number: Option<BlockNumber>) -> Result<Bytes> {
		self.call(request, number)
	}

	fn estimate_gas(
		&self,
		request: CallRequest,
		number: Option<BlockNumber>,
	) -> BoxFuture<'static, Result<U256>> {
		self.estimate_gas(request, number)
	}

	// ########################################################################
	// Fee
	// ########################################################################

	fn gas_price(&self) -> Result<U256> {
		self.gas_price()
	}

	fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistory> {
		self.fee_history(block_count, newest_block, reward_percentiles)
	}

	fn max_priority_fee_per_gas(&self) -> Result<U256> {
		self.max_priority_fee_per_gas()
	}

	// ########################################################################
	// Mining
	// ########################################################################

	fn is_mining(&self) -> Result<bool> {
		self.is_mining()
	}

	fn hashrate(&self) -> Result<U256> {
		self.hashrate()
	}

	fn work(&self) -> Result<Work> {
		self.work()
	}

	fn submit_hashrate(&self, hashrate: U256, id: H256) -> Result<bool> {
		self.submit_hashrate(hashrate, id)
	}

	fn submit_work(&self, nonce: H64, pow_hash: H256, mix_digest: H256) -> Result<bool> {
		self.submit_work(nonce, pow_hash, mix_digest)
	}

	// ########################################################################
	// Submit
	// ########################################################################

	fn send_transaction(&self, request: TransactionRequest) -> BoxFuture<'static, Result<H256>> {
		self.send_transaction(request)
	}

	fn send_raw_transaction(&self, bytes: Bytes) -> BoxFuture<'static, Result<H256>> {
		self.send_raw_transaction(bytes)
	}
}

fn rich_block_build(
	block: EthereumBlock,
	statuses: Vec<Option<TransactionStatus>>,
	hash: Option<H256>,
	full_transactions: bool,
	base_fee: Option<U256>,
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
	block: Option<EthereumBlock>,
	status: Option<TransactionStatus>,
	base_fee: Option<U256>,
) -> Transaction {
	let mut transaction: Transaction = ethereum_transaction.clone().into();

	if let EthereumTransaction::EIP1559(_) = ethereum_transaction {
		if block.is_none() && status.is_none() {
			// If transaction is not mined yet, gas price is considered just max fee per gas.
			transaction.gas_price = transaction.max_fee_per_gas;
		} else {
			let base_fee = base_fee.unwrap_or_default();
			let max_priority_fee_per_gas = transaction.max_priority_fee_per_gas.unwrap_or_default();
			let max_fee_per_gas = transaction.max_fee_per_gas.unwrap_or_default();
			// If transaction is already mined, gas price is the effective gas price.
			transaction.gas_price = Some(
				base_fee
					.checked_add(max_priority_fee_per_gas)
					.unwrap_or(U256::max_value())
					.min(max_fee_per_gas),
			);
		}
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
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
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
