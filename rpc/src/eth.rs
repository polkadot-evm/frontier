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
use sp_consensus::SelectChain;
use sp_transaction_pool::TransactionPool;
use sc_client_api::backend::{StorageProvider, Backend, StateBackend, AuxStore};
use sha3::{Keccak256, Digest};
use sp_runtime::traits::BlakeTwo256;
use sp_blockchain::{Error as BlockChainError, HeaderMetadata, HeaderBackend};
use frontier_rpc_core::{EthApi as EthApiT, NetApi as NetApiT};
use frontier_rpc_core::types::{
	BlockNumber, Bytes, CallRequest, Filter, Index, Log, Receipt, RichBlock,
	SyncStatus, SyncInfo, Transaction, Work, Rich, Block, BlockTransactions, VariadicValue
};
use frontier_rpc_primitives::{EthereumRuntimeRPCApi, ConvertTransaction, TransactionStatus};
use crate::internal_err;

pub use frontier_rpc_core::{EthApiServer, NetApiServer};

pub struct EthApi<B: BlockT, C, SC, P, CT, BE> {
	pool: Arc<P>,
	client: Arc<C>,
	select_chain: SC,
	convert_transaction: CT,
	is_authority: bool,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, C, SC, P, CT, BE> EthApi<B, C, SC, P, CT, BE> {
	pub fn new(
		client: Arc<C>,
		select_chain: SC,
		pool: Arc<P>,
		convert_transaction: CT,
		is_authority: bool
	) -> Self {
		Self { client, select_chain, pool, convert_transaction, is_authority, _marker: PhantomData }
	}
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

impl<B, C, SC, P, CT, BE> EthApi<B, C, SC, P, CT, BE> where
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
	SC: SelectChain<B> + Clone + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
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
					self.select_chain.best_chain()
						.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?
						.hash()
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
}

impl<B, C, SC, P, CT, BE> EthApiT for EthApi<B, C, SC, P, CT, BE> where
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
	SC: SelectChain<B> + Clone + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
{
	fn protocol_version(&self) -> Result<u64> {
		Ok(1)
	}

	fn syncing(&self) -> Result<SyncStatus> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?;

		let block_number = U256::from(header.number().clone().unique_saturated_into());

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
		let header = self.select_chain
			.best_chain()
			.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?;

		Ok(
			self.client
			.runtime_api()
			.author(&BlockId::Hash(header.hash()))
			.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?.into()
		)
	}

	fn is_mining(&self) -> Result<bool> {
		Ok(self.is_authority)
	}

	fn chain_id(&self) -> Result<Option<U64>> {
		let header = self.select_chain.best_chain()
			.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?;
		Ok(Some(self.client.runtime_api().chain_id(&BlockId::Hash(header.hash()))
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?.into()))
	}

	fn gas_price(&self) -> Result<U256> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?;
		Ok(
			self.client
				.runtime_api()
				.gas_price(&BlockId::Hash(header.hash()))
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.into(),
		)
	}

	fn accounts(&self) -> Result<Vec<H160>> {
		Ok(vec![])
	}

	fn block_number(&self) -> Result<U256> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?;
		Ok(U256::from(header.number().clone().unique_saturated_into()))
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

		let block = self.client.runtime_api().current_block(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;
		let statuses = self.client.runtime_api().current_transaction_statuses(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

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

		let block = self.client.runtime_api().current_block(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;
		let statuses = self.client.runtime_api().current_transaction_statuses(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

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

		let block = self.client.runtime_api()
			.current_block(&id)
			.map_err(|err| internal_err(format!("fetch runtime account basic failed: {:?}", err)))?;

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

		let block = self.client.runtime_api()
			.current_block(&id)
			.map_err(|err| internal_err(format!("fetch runtime account basic failed: {:?}", err)))?;

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
		let transaction_hash = H256::from_slice(
			Keccak256::digest(&rlp::encode(&transaction)).as_slice()
		);
		let header = match self.select_chain.best_chain() {
			Ok(header) => header,
			Err(_) => return Box::new(
				future::result(Err(internal_err("fetch header failed")))
			),
		};
		let best_block_hash = header.hash();
		Box::new(
			self.pool
				.submit_one(
					&BlockId::hash(best_block_hash),
					TransactionSource::Local,
					self.convert_transaction.convert_transaction(transaction),
				)
				.compat()
				.map(move |_| transaction_hash)
				.map_err(|err| internal_err(format!("submit transaction to pool failed: {:?}", err)))
		)
	}

	fn call(&self, request: CallRequest, _: Option<BlockNumber>) -> Result<Bytes> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?;

		let from = request.from.unwrap_or_default();
		let to = request.to.unwrap_or_default();
		let gas_price = request.gas_price;
		let gas_limit = request.gas.unwrap_or(U256::max_value());
		let value = request.value.unwrap_or_default();
		let data = request.data.map(|d| d.0).unwrap_or_default();
		let nonce = request.nonce;

		let (ret, _) = self.client.runtime_api()
			.call(
				&BlockId::Hash(header.hash()),
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
		let header = self
			.select_chain
			.best_chain()
			.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?;

		let from = request.from.unwrap_or_default();
		let gas_price = request.gas_price;
		let gas_limit = request.gas.unwrap_or(U256::max_value()); // TODO: this isn't safe
		let value = request.value.unwrap_or_default();
		let data = request.data.map(|d| d.0).unwrap_or_default();
		let nonce = request.nonce;

		let (_, used_gas) = self.client.runtime_api()
			.call(
				&BlockId::Hash(header.hash()),
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

		let block = self.client.runtime_api().current_block(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;
		let statuses = self.client.runtime_api().current_transaction_statuses(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

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
		let index = index.value();

		let block = self.client.runtime_api().current_block(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;
		let statuses = self.client.runtime_api().current_transaction_statuses(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

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

		let block = self.client.runtime_api().current_block(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;
		let statuses = self.client.runtime_api().current_transaction_statuses(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

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

		let block = self.client.runtime_api().current_block(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;
		let receipts = self.client.runtime_api().current_receipts(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;
		let statuses = self.client.runtime_api().current_transaction_statuses(&id)
			.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

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
		let mut blocks_and_receipts = Vec::new();
		let mut ret = Vec::new();

		if let Some(hash) = filter.block_hash {
			let id = match self.load_hash(hash)
				.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(Vec::new()),
			};

			let block = self.client.runtime_api()
				.current_block(&id)
				.map_err(|err| internal_err(format!("fetch runtime account basic failed: {:?}", err)))?;
			let receipts = self.client.runtime_api().current_receipts(&id)
				.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

			if let (Some(block), Some(receipts)) = (block, receipts) {
				blocks_and_receipts.push((block, receipts));
			}
		} else {
			let mut current_number = filter.to_block
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(
					*self.select_chain.best_chain()
						.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?
						.number()
				);

			let from_number = filter.from_block
				.and_then(|v| v.to_min_block_num())
				.map(|s| s.unique_saturated_into())
				.unwrap_or(
					*self.select_chain.best_chain()
						.map_err(|err| internal_err(format!("fetch header failed: {:?}", err)))?
						.number()
				);

			while current_number >= from_number {
				let id = BlockId::Number(current_number);

				let block = self.client.runtime_api()
					.current_block(&id)
					.map_err(|err| internal_err(format!("fetch runtime account basic failed: {:?}", err)))?;
				let receipts = self.client.runtime_api().current_receipts(&id)
					.map_err(|err| internal_err(format!("call runtime failed: {:?}", err)))?;

				if let (Some(block), Some(receipts)) = (block, receipts) {
					blocks_and_receipts.push((block, receipts));
				}

				if current_number == Zero::zero() {
					break
				} else {
					current_number = current_number.saturating_sub(One::one());
				}
			}
		}

		for (block, receipts) in blocks_and_receipts {
			let mut block_log_index: u32 = 0;
			for (index, receipt) in receipts.iter().enumerate() {
				let logs = receipt.logs.clone();
				let mut transaction_log_index: u32 = 0;
				let transaction = &block.transactions[index as usize];
				let transaction_hash = H256::from_slice(
					Keccak256::digest(&rlp::encode(transaction)).as_slice()
				);
				for log in logs {
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
					}
					if add {
						ret.push(Log {
							address: log.address.clone(),
							topics: log.topics.clone(),
							data: Bytes(log.data.clone()),
							block_hash: Some(H256::from_slice(
								Keccak256::digest(&rlp::encode(&block.header)).as_slice()
							)),
							block_number: Some(block.header.number.clone()),
							transaction_hash: Some(transaction_hash),
							transaction_index: Some(U256::from(index)),
							log_index: Some(U256::from(block_log_index)),
							transaction_log_index: Some(U256::from(transaction_log_index)),
							removed: false,
						});
					}
					transaction_log_index += 1;
					block_log_index += 1;
				}
			}
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

pub struct NetApi<B, BE, C, SC> {
	select_chain: SC,
	client: Arc<C>,
	_marker: PhantomData<(B, BE)>,
}

impl<B, BE, C, SC> NetApi<B, BE, C, SC> {
	pub fn new(
		client: Arc<C>,
		select_chain: SC,
	) -> Self {
		Self {
			client: client,
			select_chain: select_chain,
			_marker: PhantomData,
		}
	}
}

impl<B, BE, C, SC> NetApiT for NetApi<B, BE, C, SC> where
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + AuxStore,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	C: Send + Sync + 'static,
	SC: SelectChain<B> + Clone + 'static,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
{
	fn is_listening(&self) -> Result<bool> {
		Ok(true)
	}

	fn peer_count(&self) -> Result<String> {
		Ok("0".to_string())
	}

	fn version(&self) -> Result<String> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		Ok(self.client.runtime_api().chain_id(&BlockId::Hash(header.hash()))
			.map_err(|_| internal_err("fetch runtime chain id failed"))?.to_string())
	}
}
