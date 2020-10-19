// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Frontier.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use std::{marker::PhantomData, sync::Arc};
use std::collections::BTreeMap;
use ethereum::{Block as EthereumBlock, Transaction as EthereumTransaction};
use ethereum_types::{H160, H256, H64, U256, U64, H512};
use jsonrpc_core::{BoxFuture, Result, futures::future::{self, Future}};
use futures::future::TryFutureExt;
use sp_runtime::traits::{Block as BlockT, Header as _, UniqueSaturatedInto, Zero, One, Saturating};
use sp_runtime::transaction_validity::TransactionSource;
use sp_api::{ProvideRuntimeApi, BlockId};
use sp_transaction_pool::TransactionPool;
use sc_transaction_graph::{Pool, ChainApi};
use sc_client_api::backend::{StorageProvider, Backend, StateBackend, AuxStore};
use sha3::{Keccak256, Digest};
use sp_runtime::traits::BlakeTwo256;
use sp_blockchain::{Error as BlockChainError, HeaderMetadata, HeaderBackend};
use sp_storage::StorageKey;
use codec::Decode;
use sp_io::hashing::twox_128;
use frontier_rpc_core::{EthApi as EthApiT, NetApi as NetApiT};
use frontier_rpc_core::types::{
	BlockNumber, Bytes, CallRequest, Filter, Index, Log, Receipt, RichBlock,
	SyncStatus, SyncInfo, Transaction, Work, Rich, Block, BlockTransactions, VariadicValue
};
use frontier_rpc_primitives::{EthereumRuntimeRPCApi, ConvertTransaction, TransactionStatus};
use crate::internal_err;

pub use frontier_rpc_core::{EthApiServer, NetApiServer};

const DEFAULT_BLOCK_LIMIT: u32 = 50;
const DEFAULT_LOG_LIMIT: u32 = 500;

pub struct EthApi<B: BlockT, C, P, CT, BE, A: ChainApi> {
	pool: Arc<P>,
	graph_pool: Arc<Pool<A>>,
	client: Arc<C>,
	convert_transaction: CT,
	is_authority: bool,
	eth_block_limit: Option<u32>,
	eth_log_limit: Option<u32>,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, C, P, CT, BE, A: ChainApi> EthApi<B, C, P, CT, BE, A> {
	pub fn new(
		client: Arc<C>,
		graph_pool: Arc<Pool<A>>,
		pool: Arc<P>,
		convert_transaction: CT,
		is_authority: bool,
		eth_block_limit: Option<u32>,
		eth_log_limit: Option<u32>,
	) -> Self {
		Self { client, pool, graph_pool, convert_transaction, is_authority, eth_block_limit, eth_log_limit, _marker: PhantomData }
	}
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn rich_block_build(
	block: ethereum::Block,
	statuses: Vec<Option<TransactionStatus>>,
	hash: Option<H256>,
	full_transactions: bool
) -> RichBlock {
	Rich {
		inner: Block {
			hash: Some(hash.unwrap_or_else(|| {
				H256::from_slice(
					Keccak256::digest(&rlp::encode(&block.header)).as_slice()
				)
			})),
			parent_hash: block.header.parent_hash,
			uncles_hash: H256::zero(),
			author: block.header.beneficiary,
			miner: block.header.beneficiary,
			state_root: block.header.state_root,
			transactions_root: block.header.transactions_root,
			receipts_root: block.header.receipts_root,
			number: Some(block.header.number),
			gas_used: block.header.gas_used,
			gas_limit: block.header.gas_limit,
			extra_data: Bytes(block.header.extra_data.as_bytes().to_vec()),
			logs_bloom: Some(block.header.logs_bloom),
			timestamp: U256::from(block.header.timestamp / 1000),
			difficulty: block.header.difficulty,
			total_difficulty: None,
			seal_fields: vec![
				Bytes(block.header.mix_hash.as_bytes().to_vec()),
				Bytes(block.header.nonce.as_bytes().to_vec())
			],
			uncles: vec![],
			transactions: {
				if full_transactions {
					BlockTransactions::Full(
						block.transactions.iter().enumerate().map(|(index, transaction)|{
							transaction_build(
								transaction.clone(),
								block.clone(),
								statuses[index].clone().unwrap_or_default()
							)
						}).collect()
					)
				} else {
					BlockTransactions::Hashes(
						block.transactions.iter().map(|transaction|{
							H256::from_slice(
								Keccak256::digest(&rlp::encode(&transaction.clone())).as_slice()
							)
						}).collect()
					)
				}
			},
			size: Some(U256::from(rlp::encode(&block).len() as u32))
		},
		extra_info: BTreeMap::new()
	}
}

fn transaction_build(
	transaction: EthereumTransaction,
	block: EthereumBlock,
	status: TransactionStatus
) -> Transaction {
	let mut sig = [0u8; 65];
	let mut msg = [0u8; 32];
	sig[0..32].copy_from_slice(&transaction.signature.r()[..]);
	sig[32..64].copy_from_slice(&transaction.signature.s()[..]);
	sig[64] = transaction.signature.standard_v();
	msg.copy_from_slice(&transaction.message_hash(
		transaction.signature.chain_id().map(u64::from)
	)[..]);

	let pubkey = match sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg) {
		Ok(p) => Some(H512::from(p)),
		Err(_e) => None,
	};

	Transaction {
		hash: H256::from_slice(
			Keccak256::digest(&rlp::encode(&transaction)).as_slice()
		),
		nonce: transaction.nonce,
		block_hash: Some(H256::from_slice(
			Keccak256::digest(&rlp::encode(&block.header)).as_slice()
		)),
		block_number: Some(block.header.number),
		transaction_index: Some(U256::from(
			UniqueSaturatedInto::<u32>::unique_saturated_into(
				status.transaction_index
			)
		)),
		from: status.from,
		to: status.to,
		value: transaction.value,
		gas_price: transaction.gas_price,
		gas: transaction.gas_limit,
		input: Bytes(transaction.clone().input),
		creates: status.contract_address,
		raw: Bytes(rlp::encode(&transaction)),
		public_key: pubkey,
		chain_id: transaction.signature.chain_id().map(U64::from),
		standard_v: U256::from(transaction.signature.standard_v()),
		v: U256::from(transaction.signature.v()),
		r: U256::from(transaction.signature.r().as_bytes()),
		s: U256::from(transaction.signature.s().as_bytes()),
		condition: None // TODO
	}
}

impl<B, C, P, CT, BE, A> EthApi<B, C, P, CT, BE, A> where
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
	A: ChainApi<Block=B> + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
{
	fn native_block_id(&self, number: Option<BlockNumber>) -> Result<Option<BlockId<B>>> {
		Ok(match number.unwrap_or(BlockNumber::Latest) {
			BlockNumber::Hash { hash, .. } => {
				self.load_hash(hash).unwrap_or(None)
			},
			BlockNumber::Num(number) => {
				Some(BlockId::Number(number.unique_saturated_into()))
			},
			BlockNumber::Latest => {
				Some(BlockId::Hash(
					self.client.info().best_hash
				))
			},
			BlockNumber::Earliest => {
				Some(BlockId::Number(Zero::zero()))
			},
			BlockNumber::Pending => {
				None
			}
		})
	}

	// Asumes there is only one mapped canonical block in the AuxStore, otherwise something is wrong
	fn load_hash(&self, hash: H256) -> Result<Option<BlockId<B>>> {
		let hashes = match frontier_consensus::load_block_hash::<B, _>(self.client.as_ref(), hash)
			.map_err(|err| internal_err(format!("fetch aux store failed: {:?}", err)))?
		{
			Some(hashes) => hashes,
			None => return Ok(None),
		};
		let out: Vec<H256> = hashes.into_iter()
			.filter_map(|h| {
				if let Ok(Some(_)) = self.client.header(BlockId::Hash(h)) {
					Some(h)
				} else {
					None
				}
			}).collect();

		if out.len() == 1 {
			return Ok(Some(
				BlockId::Hash(out[0])
			));
		}
		Ok(None)
	}

	fn headers(&self, id: &BlockId<B>) -> (u64,u64) {
		let best_number: u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(
			self.client.info().best_number
		);
		let header_number: u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(
			*self.client.header(id.clone()).unwrap().unwrap().number()
		);
		(best_number, header_number)
	}

	fn current_block(&self, id: &BlockId<B>) -> Option<ethereum::Block> {
		if let Ok(Some(block_data)) = self.client.storage(
			&id,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentBlock")
			)
		) {
			return Decode::decode(&mut &block_data.0[..]).unwrap_or_else(|_| None);
		} else { return None; };
	}

	fn current_statuses(&self, id: &BlockId<B>) -> Option<Vec<TransactionStatus>> {
		if let Ok(Some(status_data)) = self.client.storage(
			&id,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentTransactionStatuses")
			)
		) {
			return Decode::decode(&mut &status_data.0[..]).unwrap_or_else(|_| None);
		} else { return None; };
	}

	fn current_receipts(&self, id: &BlockId<B>) -> Option<Vec<ethereum::Receipt>> {
		if let Ok(Some(status_data)) = self.client.storage(
			&id,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentReceipts")
			)
		) {
			return Decode::decode(&mut &status_data.0[..]).unwrap_or_else(|_| None);
		} else { return None; };
	}
}

impl<B, C, P, CT, BE, A> EthApiT for EthApi<B, C, P, CT, BE, A> where
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
	A: ChainApi<Block=B> + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
{
	fn protocol_version(&self) -> Result<u64> {
		Ok(1)
	}

	fn syncing(&self) -> Result<SyncStatus> {
		let block_number = U256::from(self.client.info().best_number.clone().unique_saturated_into());

		Ok(SyncStatus::Info(SyncInfo {
			starting_block: U256::zero(),
			current_block: block_number,
			highest_block: block_number,
			warp_chunks_amount: None,
			warp_chunks_processed: None,
		}))
	}

	fn hashrate(&self) -> Result<U256> {
		Ok(U256::zero())
	}

	fn author(&self) -> Result<H160> {
		let hash = self.client.info().best_hash;

		Ok(
			self.client
			.runtime_api()
			.author(&BlockId::Hash(hash))
			.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?.into()
		)
	}

	fn is_mining(&self) -> Result<bool> {
		Ok(self.is_authority)
	}

	fn chain_id(&self) -> Result<Option<U64>> {
		let hash = self.client.info().best_hash;
		Ok(Some(self.client.runtime_api().chain_id(&BlockId::Hash(hash))
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?.into()))
	}

	fn gas_price(&self) -> Result<U256> {
		let hash = self.client.info().best_hash;
		Ok(
			self.client
				.runtime_api()
				.gas_price(&BlockId::Hash(hash))
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.into(),
		)
	}

	fn accounts(&self) -> Result<Vec<H160>> {
		Ok(vec![])
	}

	fn block_number(&self) -> Result<U256> {
		Ok(U256::from(self.client.info().best_number.clone().unique_saturated_into()))
	}

	fn balance(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		if let Ok(Some(id)) = self.native_block_id(number) {
			return Ok(
				self.client
					.runtime_api()
					.account_basic(&id, address)
					.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
					.balance.into(),
			);
		}
		Ok(U256::zero())
	}

	fn storage_at(&self, address: H160, index: U256, number: Option<BlockNumber>) -> Result<H256> {
		if let Ok(Some(id)) = self.native_block_id(number) {
			return Ok(
				self.client
					.runtime_api()
					.storage_at(&id, address, index)
					.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
					.into(),
			);
		}
		Ok(H256::default())
	}

	fn block_by_hash(&self, hash: H256, full: bool) -> Result<Option<RichBlock>> {
		let id = match self.load_hash(hash)
			.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let (best_number, header_number) = self.headers(&id);
		if header_number > best_number {
			return Ok(None);
		}

		let block: Option<ethereum::Block> = self.current_block(&id);
		let statuses: Option<Vec<TransactionStatus>> = self.current_statuses(&id);

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				Ok(Some(rich_block_build(
					block,
					statuses.into_iter().map(|s| Some(s)).collect(),
					Some(hash),
					full,
				)))
			},
			_ => {
				Ok(None)
			},
		}
	}

	fn block_by_number(&self, number: BlockNumber, full: bool) -> Result<Option<RichBlock>> {
		let id = match self.native_block_id(Some(number))? {
			Some(id) => id,
			None => return Ok(None),
		};

		let block: Option<ethereum::Block> = self.current_block(&id);
		let statuses: Option<Vec<TransactionStatus>> = self.current_statuses(&id);

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				let hash = H256::from_slice(
					Keccak256::digest(&rlp::encode(&block.header)).as_slice(),
				);

				Ok(Some(rich_block_build(
					block,
					statuses.into_iter().map(|s| Some(s)).collect(),
					Some(hash),
					full,
				)))
			},
			_ => {
				Ok(None)
			},
		}
	}

	fn transaction_count(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		let id = match self.native_block_id(number)? {
			Some(id) => id,
			None => return Ok(U256::zero()),
		};

		let nonce = self.client.runtime_api()
			.account_basic(&id, address)
			.map_err(|err| internal_err(format!("fetch runtime account basic failed: {:?}", err)))?
			.nonce.into();

		Ok(nonce)
	}

	fn block_transaction_count_by_hash(&self, hash: H256) -> Result<Option<U256>> {
		let id = match self.load_hash(hash)
			.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let (best_number, header_number) = self.headers(&id);
		if header_number > best_number {
			return Ok(None);
		}

		let block: Option<ethereum::Block> = self.current_block(&id);

		match block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	fn block_transaction_count_by_number(&self, number: BlockNumber) -> Result<Option<U256>> {
		let id = match self.native_block_id(Some(number))? {
			Some(id) => id,
			None => return Ok(None),
		};

		let block: Option<ethereum::Block> = self.current_block(&id);

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
		if let Ok(Some(id)) = self.native_block_id(number) {
			return Ok(
				self.client
					.runtime_api()
					.account_code_at(&id, address)
					.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
					.into(),
			);
		}
		Ok(Bytes(vec![]))
	}

	fn send_raw_transaction(&self, bytes: Bytes) -> BoxFuture<H256> {
		let transaction = match rlp::decode::<ethereum::Transaction>(&bytes.0[..]) {
			Ok(transaction) => transaction,
			Err(_) => return Box::new(
				future::result(Err(internal_err("decode transaction failed")))
			),
		};
		// pre-submit checks
		let uxt = self.convert_transaction.convert_transaction(transaction.clone());
		let (uxt_hash, _bytes) = self.graph_pool.validated_pool().api().hash_and_length(&uxt);
		let check_is_known = self.graph_pool.validated_pool().check_is_known(&uxt_hash,false);

		match check_is_known {
			Ok(_) => {
				let hash = self.client.info().best_hash;
				let transaction_hash = H256::from_slice(
					Keccak256::digest(&rlp::encode(&transaction)).as_slice()
				);
				Box::new(
					self.pool
						.submit_one(
							&BlockId::hash(hash),
							TransactionSource::Local,
							uxt,
						)
						.compat()
						.map(move |_| transaction_hash)
						.map_err(|err| internal_err(format!("submit transaction to pool failed: {:?}", err)))
				)
			},
			_ => {
				// Transaction is already imported or in the ban list
				Box::new(
					futures::future::err::<_, jsonrpc_core::types::error::Error>(
						internal_err(format!("{:?}",check_is_known))
					).compat()
				)
			}
		}
	}

	fn call(&self, request: CallRequest, _: Option<BlockNumber>) -> Result<Bytes> {
		let hash = self.client.info().best_hash;

		let from = request.from.unwrap_or_default();
		let to = request.to.unwrap_or_default();
		let gas_price = request.gas_price;
		let gas_limit = request.gas.unwrap_or(U256::max_value());
		let value = request.value.unwrap_or_default();
		let data = request.data.map(|d| d.0).unwrap_or_default();
		let nonce = request.nonce;

		let (ret, _) = self.client.runtime_api()
			.call(
				&BlockId::Hash(hash),
				from,
				data,
				value,
				gas_limit,
				gas_price,
				nonce,
				ethereum::TransactionAction::Call(to)
			)
			.map_err(|err| internal_err(format!("internal error: {:?}", err)))?
			.map_err(|err| internal_err(format!("executing call failed: {:?}", err)))?;

		Ok(Bytes(ret))
	}

	fn estimate_gas(&self, request: CallRequest, _: Option<BlockNumber>) -> Result<U256> {
		let hash = self.client.info().best_hash;

		let from = request.from.unwrap_or_default();
		let gas_price = request.gas_price;
		let gas_limit = request.gas.unwrap_or(U256::max_value()); // TODO: this isn't safe
		let value = request.value.unwrap_or_default();
		let data = request.data.map(|d| d.0).unwrap_or_default();
		let nonce = request.nonce;

		let (_, used_gas) = self.client.runtime_api()
			.call(
				&BlockId::Hash(hash),
				from,
				data,
				value,
				gas_limit,
				gas_price,
				nonce,
				match request.to {
					Some(to) => ethereum::TransactionAction::Call(to),
					_ => ethereum::TransactionAction::Create,
				}
			)
			.map_err(|err| internal_err(format!("internal error: {:?}", err)))?
			.map_err(|err| internal_err(format!("executing call failed: {:?}", err)))?;

		Ok(used_gas)
	}

	fn transaction_by_hash(&self, hash: H256) -> Result<Option<Transaction>> {
		let (hash, index) = match frontier_consensus::load_transaction_metadata(
			self.client.as_ref(),
			hash,
		).map_err(|err| internal_err(format!("fetch aux store failed: {:?})", err)))? {
			Some((hash, index)) => (hash, index as usize),
			None => return Ok(None),
		};

		let id = match self.load_hash(hash)
			.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let (best_number, header_number) = self.headers(&id);
		if header_number > best_number {
			return Ok(None);
		}

		let block: Option<ethereum::Block> = self.current_block(&id);
		let statuses: Option<Vec<TransactionStatus>> = self.current_statuses(&id);

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				Ok(Some(transaction_build(
					block.transactions[index].clone(),
					block,
					statuses[index].clone(),
				)))
			},
			_ => Ok(None)
		}
	}

	fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> Result<Option<Transaction>> {
		let id = match self.load_hash(hash)
			.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let (best_number, header_number) = self.headers(&id);
		if header_number > best_number {
			return Ok(None);
		}
		let index = index.value();

		let block: Option<ethereum::Block> = self.current_block(&id);
		let statuses: Option<Vec<TransactionStatus>> = self.current_statuses(&id);

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				Ok(Some(transaction_build(
					block.transactions[index].clone(),
					block,
					statuses[index].clone(),
				)))
			},
			_ => Ok(None)
		}
	}

	fn transaction_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> Result<Option<Transaction>> {
		let id = match self.native_block_id(Some(number))? {
			Some(id) => id,
			None => return Ok(None),
		};
		let index = index.value();

		let block: Option<ethereum::Block> = self.current_block(&id);
		let statuses: Option<Vec<TransactionStatus>> = self.current_statuses(&id);

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				Ok(Some(transaction_build(
					block.transactions[index].clone(),
					block,
					statuses[index].clone(),
				)))
			},
			_ => Ok(None)
		}
	}

	fn transaction_receipt(&self, hash: H256) -> Result<Option<Receipt>> {
		let (hash, index) = match frontier_consensus::load_transaction_metadata(
			self.client.as_ref(),
			hash,
		).map_err(|err| internal_err(format!("fetch aux store failed : {:?}", err)))? {
			Some((hash, index)) => (hash, index as usize),
			None => return Ok(None),
		};

		let id = match self.load_hash(hash)
			.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let (best_number, header_number) = self.headers(&id);
		if header_number > best_number {
			return Ok(None);
		}

		let block: Option<ethereum::Block> = self.current_block(&id);
		let statuses: Option<Vec<TransactionStatus>> = self.current_statuses(&id);
		let receipts: Option<Vec<ethereum::Receipt>> = self.current_receipts(&id);

		match (block, statuses, receipts) {
			(Some(block), Some(statuses), Some(receipts)) => {
				let block_hash = H256::from_slice(
					Keccak256::digest(&rlp::encode(&block.header)).as_slice()
				);
				let receipt = receipts[index].clone();
				let status = statuses[index].clone();
				let mut cumulative_receipts = receipts.clone();
				cumulative_receipts.truncate((status.transaction_index + 1) as usize);

				return Ok(Some(Receipt {
					transaction_hash: Some(status.transaction_hash),
					transaction_index: Some(status.transaction_index.into()),
					block_hash: Some(block_hash),
					from: Some(status.from),
					to: status.to,
					block_number: Some(block.header.number),
					cumulative_gas_used: {
						let cumulative_gas: u32 = cumulative_receipts.iter().map(|r| {
							r.used_gas.as_u32()
						}).sum();
						U256::from(cumulative_gas)
					},
					gas_used: Some(receipt.used_gas),
					contract_address: status.contract_address,
					logs: {
						let mut pre_receipts_log_index = None;
						if cumulative_receipts.len() > 0 {
							cumulative_receipts.truncate(cumulative_receipts.len() - 1);
							pre_receipts_log_index = Some(cumulative_receipts.iter().map(|r| {
								r.logs.len() as u32
							}).sum::<u32>());
						}
						receipt.logs.iter().enumerate().map(|(i, log)| {
							Log {
								address: log.address,
								topics: log.topics.clone(),
								data: Bytes(log.data.clone()),
								block_hash: Some(block_hash),
								block_number: Some(block.header.number),
								transaction_hash: Some(hash),
								transaction_index: Some(status.transaction_index.into()),
								log_index: Some(U256::from(
									(pre_receipts_log_index.unwrap_or(0)) + i as u32
								)),
								transaction_log_index: Some(U256::from(i)),
								removed: false,
							}
						}).collect()
					},
					state_root: Some(receipt.state_root),
					logs_bloom: receipt.logs_bloom,
					status_code: None,
				}))
			}
			_ => Ok(None),
		}
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

	fn logs(&self, filter: Filter) -> Result<Vec<Log>> {
		let mut blocks_and_statuses = Vec::new();
		let mut ret = Vec::new();

		// Check for number of past blocks allowed for querying ethereum events.
		let mut eth_block_limit: u32 = DEFAULT_BLOCK_LIMIT;
		// Check for number of logs allowed for querying ethereum events.
		let mut eth_log_limit: u32 = DEFAULT_LOG_LIMIT;
		if let Some(block_limit) = self.eth_block_limit {
			eth_block_limit = block_limit;
		}
		if let Some(log_limit) = self.eth_log_limit {
			eth_log_limit = log_limit;
		}

		if let Some(hash) = filter.block_hash {
			let id = match self.load_hash(hash)
				.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(Vec::new()),
			};
			let (best_number, header_number) = self.headers(&id);
			if header_number > best_number {
				return Ok(Vec::new());
			}

			let block: Option<ethereum::Block> = self.current_block(&id);
			let statuses: Option<Vec<TransactionStatus>> = self.current_statuses(&id);

			let block_hash = Some(H256::from_slice(
				Keccak256::digest(&rlp::encode(&block.clone().unwrap().header)).as_slice()
			));
			let block_number = Some(block.unwrap().header.number);

			if let (Some(block_hash), Some(block_number), Some(statuses)) = (block_hash, block_number, statuses) {
				blocks_and_statuses.push((block_hash, block_number, statuses));
			}
		} else {
			let mut current_number = filter.to_block
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(
					self.client.info().best_number
				);

			let from_number = filter.from_block
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(
					self.client.info().best_number
				);
			while current_number >= from_number {
				let number = UniqueSaturatedInto::<u32>::unique_saturated_into(current_number);

				match frontier_consensus::load_logs(
					self.client.as_ref(),
					number
				).map_err(|err| internal_err(format!("fetch aux store failed: {:?}", err)))?
				{
					Some((block_hash, statuses)) => {
						let block_number = U256::from(
							UniqueSaturatedInto::<u32>::unique_saturated_into(current_number)
						);
						blocks_and_statuses.push((block_hash, block_number, statuses));
					},
					_ => {},
				};

				if current_number == Zero::zero() {
					break
				} else {
					current_number = current_number.saturating_sub(One::one());
				}
			}
		}
		
		let mut blocks_processed: u32 = 0;
		let mut logs_processed: u32 = 0;
		
		'outer: for (block_hash, block_number, statuses) in blocks_and_statuses {
			if blocks_processed == eth_block_limit {
				break;
			}
			let mut block_log_index: u32 = 0;
			for status in statuses.iter() {
				let logs = status.logs.clone();
				let mut transaction_log_index: u32 = 0;
				let transaction_hash = status.transaction_hash;
				for log in logs {
					if logs_processed == eth_log_limit {
						break 'outer;
					}
					let mut add: bool = false;
					if let (
						Some(VariadicValue::Single(address)),
						Some(VariadicValue::Multiple(topics))
					) = (
						filter.address.clone(),
						filter.topics.clone(),
					) {
						if address == log.address && log.topics.starts_with(&topics) {
							add = true;
						}
					} else if let Some(VariadicValue::Single(address)) = filter.address {
						if address == log.address {
							add = true;
						}
					} else if let Some(VariadicValue::Multiple(topics)) = &filter.topics {
						if log.topics.starts_with(&topics) {
							add = true;
						}
					} else {
						add = true;
					}
					if add {
						ret.push(Log {
							address: log.address.clone(),
							topics: log.topics.clone(),
							data: Bytes(log.data.clone()),
							block_hash: Some(block_hash.clone()),
							block_number: Some(block_number.clone()),
							transaction_hash: Some(transaction_hash),
							transaction_index: Some(U256::from(status.transaction_index)),
							log_index: Some(U256::from(block_log_index)),
							transaction_log_index: Some(U256::from(transaction_log_index)),
							removed: false,
						});
					}
					transaction_log_index += 1;
					block_log_index += 1;
					logs_processed += 1;
				}
			}
			blocks_processed += 1;
		}

		Ok(ret)
	}

	fn work(&self) -> Result<Work> {
		Ok(Work {
			pow_hash: H256::default(),
			seed_hash: H256::default(),
			target: H256::default(),
			number: None,
		})
	}

	fn submit_work(&self, _: H64, _: H256, _: H256) -> Result<bool> {
		Ok(false)
	}

	fn submit_hashrate(&self, _: U256, _: H256) -> Result<bool> {
		Ok(false)
	}
}

pub struct NetApi<B, BE, C> {
	client: Arc<C>,
	_marker: PhantomData<(B, BE)>,
}

impl<B, BE, C> NetApi<B, BE, C> {
	pub fn new(
		client: Arc<C>,
	) -> Self {
		Self {
			client: client,
			_marker: PhantomData,
		}
	}
}

impl<B, BE, C> NetApiT for NetApi<B, BE, C> where
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	C: Send + Sync + 'static,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
{
	fn is_listening(&self) -> Result<bool> {
		Ok(true)
	}

	fn peer_count(&self) -> Result<String> {
		Ok("0".to_string())
	}

	fn version(&self) -> Result<String> {
		let hash = self.client.info().best_hash;
		Ok(self.client.runtime_api().chain_id(&BlockId::Hash(hash))
			.map_err(|_| internal_err("fetch runtime chain id failed"))?.to_string())
	}
}
