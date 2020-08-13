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
use ethereum_types::{H160, H256, H64, U256, U64};
use jsonrpc_core::{BoxFuture, Result, ErrorCode, Error, futures::future::{self, Future}};
use futures::future::TryFutureExt;
use sp_runtime::traits::{Block as BlockT, Header as _, UniqueSaturatedInto};
use sp_runtime::transaction_validity::TransactionSource;
use sp_api::{ProvideRuntimeApi, BlockId};
use sp_consensus::SelectChain;
use sp_transaction_pool::TransactionPool;
use sc_client_api::backend::{StorageProvider, Backend, StateBackend};
use sha3::{Keccak256, Digest};
use sp_runtime::traits::BlakeTwo256;
use frontier_rpc_core::EthApi as EthApiT;
use frontier_rpc_core::types::{
	BlockNumber, Bytes, CallRequest, EthAccount, Filter, Index, Log, Receipt, RichBlock,
	SyncStatus, Transaction, Work, Rich, Block, BlockTransactions, VariadicValue
};
use frontier_rpc_primitives::{EthereumRuntimeApi, ConvertTransaction, TransactionStatus};

pub use frontier_rpc_core::EthApiServer;

fn internal_err(message: &str) -> Error {
	Error {
		code: ErrorCode::InternalError,
		message: message.to_string(),
		data: None
	}
}
fn not_supported_err(message: &str) -> Error {
	Error {
		code: ErrorCode::InvalidRequest,
		message: message.to_string(),
		data: None
	}
}

pub struct EthApi<B: BlockT, C, SC, P, CT, BE> {
	pool: Arc<P>,
	client: Arc<C>,
	select_chain: SC,
	convert_transaction: CT,
	is_authority: bool,
	_marker: PhantomData<(B,BE)>,
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
		public_key: None, // TODO
		chain_id: transaction.signature.chain_id().map(U64::from),
		standard_v: U256::from(transaction.signature.standard_v()),
		v: U256::from(transaction.signature.v()),
		r: U256::from(transaction.signature.r().as_bytes()),
		s: U256::from(transaction.signature.s().as_bytes()),
		condition: None // TODO
	}
}

impl<B, C, SC, P, CT, BE> EthApi<B, C, SC, P, CT, BE> where
	C: ProvideRuntimeApi<B> + StorageProvider<B,BE>,
	C::Api: EthereumRuntimeApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
	SC: SelectChain<B> + Clone + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
{
	fn native_block_number(&self, number: Option<BlockNumber>) -> Result<Option<u32>> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let mut native_number: Option<u32> = None;

		if let Some(number) = number {
			match number {
				BlockNumber::Hash { hash, .. } => {
					if let Ok(Some(block)) = self.client.runtime_api().block_by_hash(
						&BlockId::Hash(header.hash()),
						hash
					) {
						native_number = Some(block.header.number.as_u32());
					}
				},
				BlockNumber::Num(_) => {
					if let Some(number) = number.to_min_block_num() {
						native_number = Some(number.unique_saturated_into());
					}
				},
				BlockNumber::Latest => {
					native_number = Some(
						header.number().clone().unique_saturated_into() as u32
					);
				},
				BlockNumber::Earliest => {
					native_number = Some(0);
				},
				BlockNumber::Pending => {
					native_number = None;
				}
			};
		} else {
			native_number = Some(
				header.number().clone().unique_saturated_into() as u32
			);
		}
		Ok(native_number)
	}
}

impl<B, C, SC, P, CT, BE> EthApiT for EthApi<B, C, SC, P, CT, BE> where
	C: ProvideRuntimeApi<B> + StorageProvider<B,BE>,
	C::Api: EthereumRuntimeApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
	SC: SelectChain<B> + Clone + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
{
	/// Returns protocol version encoded as a string (quotes are necessary).
	fn protocol_version(&self) -> Result<String> {
		unimplemented!("protocol version");
	}

	fn syncing(&self) -> Result<SyncStatus> {
		unimplemented!("syncing");
	}

	fn hashrate(&self) -> Result<U256> {
		Ok(U256::zero())
	}

	fn author(&self) -> Result<H160> {
		let header = self.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		Ok(
			self.client
			.runtime_api()
			.author(&BlockId::Hash(header.hash()))
			.map_err(|_| internal_err("fetch runtime chain id failed"))?.into()
		)
	}

	fn is_mining(&self) -> Result<bool> {
		Ok(self.is_authority)
	}

	fn chain_id(&self) -> Result<Option<U64>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		Ok(Some(self.client.runtime_api().chain_id(&BlockId::Hash(header.hash()))
				.map_err(|_| internal_err("fetch runtime chain id failed"))?.into()))
	}

	fn gas_price(&self) -> Result<U256> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		Ok(
			self.client
				.runtime_api()
				.gas_price(&BlockId::Hash(header.hash()))
				.map_err(|_| internal_err("fetch runtime chain id failed"))?
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
			.map_err(|_| internal_err("fetch header failed"))?;
		Ok(U256::from(header.number().clone().unique_saturated_into()))
	}

	fn balance(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		if let Ok(Some(native_number)) = self.native_block_number(number) {
			return Ok(
				self.client
					.runtime_api()
					.account_basic(&BlockId::Number(native_number.into()), address)
					.map_err(|_| internal_err("fetch runtime chain id failed"))?
					.balance.into(),
			);
		}
		Ok(U256::zero())
	}

	fn proof(&self, _: H160, _: Vec<H256>, _: Option<BlockNumber>) -> BoxFuture<EthAccount> {
		unimplemented!("proof");
	}

	fn storage_at(&self, address: H160, index: U256, number: Option<BlockNumber>) -> Result<H256> {
		if let Ok(Some(native_number)) = self.native_block_number(number) {
			return Ok(
				self.client
					.runtime_api()
					.storage_at(&BlockId::Number(native_number.into()), address, index)
					.map_err(|_| internal_err("fetch runtime chain id failed"))?
					.into(),
			);
		}
		Ok(H256::default())
	}

	fn block_by_hash(&self, hash: H256, full: bool) -> Result<Option<RichBlock>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		if let Ok((Some(block), statuses)) = self.client.runtime_api().block_by_hash_with_statuses(
			&BlockId::Hash(header.hash()),
			hash
		) {
			Ok(Some(rich_block_build(block, statuses, Some(hash), full)))
		} else {
			Ok(None)
		}
	}

	fn block_by_number(&self, number: BlockNumber, full: bool) -> Result<Option<RichBlock>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		if let Ok(Some(native_number)) = self.native_block_number(Some(number)) {
			if let Ok((Some(block), statuses)) = self.client.runtime_api().block_by_number(
				&BlockId::Hash(header.hash()),
				native_number
			) {
				return Ok(Some(rich_block_build(block, statuses, None, full)));
			}
		}
		Ok(None)
	}

	fn transaction_count(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		if let Ok(Some(native_number)) = self.native_block_number(number) {
			return Ok(
				self.client
					.runtime_api()
					.account_basic(&BlockId::Number(native_number.into()), address)
					.map_err(|_| internal_err("fetch runtime account basic failed"))?
					.nonce.into()
			);
		}
		Ok(U256::zero())
	}

	fn block_transaction_count_by_hash(&self, hash: H256) -> Result<Option<U256>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let result = match self.client.runtime_api()
			.block_transaction_count_by_hash(&BlockId::Hash(header.hash()), hash) {
			Ok(result) => result,
			Err(_) => return Ok(None)
		};
		Ok(result)
	}

	fn block_transaction_count_by_number(&self, number: BlockNumber) -> Result<Option<U256>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let mut result = None;
		if let Ok(Some(native_number)) = self.native_block_number(Some(number)) {
			result = match self.client.runtime_api()
				.block_transaction_count_by_number(&BlockId::Hash(header.hash()), native_number) {
				Ok(result) => result,
				Err(_) => None
			};
		}
		Ok(result)
	}

	fn block_uncles_count_by_hash(&self, _: H256) -> Result<U256> {
		Ok(U256::zero())
	}

	fn block_uncles_count_by_number(&self, _: BlockNumber) -> Result<U256> {
		Ok(U256::zero())
	}

	fn code_at(&self, address: H160, number: Option<BlockNumber>) -> Result<Bytes> {
		if let Ok(Some(native_number)) = self.native_block_number(number) {
			return Ok(
				self.client
					.runtime_api()
					.account_code_at(&BlockId::Number(native_number.into()), address)
					.map_err(|_| internal_err("fetch runtime chain id failed"))?
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
				.map_err(|_| internal_err("submit transaction to pool failed"))
		)
	}

	fn submit_transaction(&self, _: Bytes) -> Result<H256> {
		unimplemented!("submit_transaction");
	}

	fn call(&self, request: CallRequest, _: Option<BlockNumber>) -> Result<Bytes> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

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
			.map_err(|_| internal_err("executing call failed"))?
			.ok_or(internal_err("inner executing call failed"))?;

		Ok(Bytes(ret))
	}

	fn estimate_gas(&self, request: CallRequest, _: Option<BlockNumber>) -> Result<U256> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let from = request.from.unwrap_or_default();
		let gas_price = request.gas_price;
		let gas_limit = request.gas.unwrap_or(U256::max_value());
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
			.map_err(|_| internal_err("executing call failed"))?
			.ok_or(internal_err("inner executing call failed"))?;

		Ok(used_gas)
	}

	fn transaction_by_hash(&self, hash: H256) -> Result<Option<Transaction>> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		if let Ok(Some((transaction, block, status, _receipt))) = self.client.runtime_api()
			.transaction_by_hash(&BlockId::Hash(header.hash()), hash) {
			return Ok(Some(transaction_build(
				transaction,
				block,
				status
			)));
		}
		Ok(None)
	}

	fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> Result<Option<Transaction>> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let index_param = index.value() as u32;

		if let Ok(Some((transaction, block, status))) = self.client.runtime_api()
			.transaction_by_block_hash_and_index(&BlockId::Hash(header.hash()), hash, index_param) {
			return Ok(Some(transaction_build(
				transaction,
				block,
				status
			)));
		}
		Ok(None)
	}

	fn transaction_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> Result<Option<Transaction>> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let index_param = index.value() as u32;

		if let Ok(Some(native_number)) = self.native_block_number(Some(number)) {
			if let Ok(Some((transaction, block, status))) = self.client.runtime_api()
				.transaction_by_block_number_and_index(
					&BlockId::Hash(header.hash()),
					native_number,
					index_param) {
				return Ok(Some(transaction_build(
					transaction,
					block,
					status
				)));
			}
		}
		Ok(None)
	}

	fn transaction_receipt(&self, hash: H256) -> Result<Option<Receipt>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		if let Ok(Some((_transaction, block, status, receipts))) = self.client.runtime_api()
			.transaction_by_hash(&BlockId::Hash(header.hash()), hash) {

			let block_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(&block.header)).as_slice()
			);
			let receipt = receipts[status.transaction_index as usize].clone();
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
		Ok(None)
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

	fn compilers(&self) -> Result<Vec<String>> {
		Err(not_supported_err("Method eth_getCompilers not supported."))
	}

	fn compile_lll(&self, _: String) -> Result<Bytes> {
		Err(not_supported_err("Method eth_compileLLL not supported."))
	}

	fn compile_solidity(&self, _: String) -> Result<Bytes> {
		Err(not_supported_err("Method eth_compileSolidity not supported."))
	}

	fn compile_serpent(&self, _: String) -> Result<Bytes> {
		Err(not_supported_err("Method eth_compileSerpent not supported."))
	}

	fn logs(&self, filter: Filter) -> Result<Vec<Log>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let mut from_block = None;
		if let Some(from_block_input) = filter.from_block {
			if let Ok(Some(block_number)) = self.native_block_number(Some(from_block_input)) {
				from_block = Some(block_number);
			}
		}

		let mut to_block = None;
		if let Some(to_block_input) = filter.to_block {
			if let Ok(Some(block_number)) = self.native_block_number(Some(to_block_input)) {
				to_block = Some(block_number);
			}
		}

		let mut address = None;
		if let Some(address_input) = filter.address {
			match address_input {
				VariadicValue::Single(x) => { address = Some(x); },
				_ => { address = None; }
			}
		}

		let mut topics = None;
		if let Some(topics_input) = filter.topics {
			match topics_input {
				VariadicValue::Multiple(x) => { topics = Some(x); },
				_ => { topics = None; }
			}
		}

		if let Ok(logs) = self.client.runtime_api()
			.logs(
				&BlockId::Hash(header.hash()),
				from_block,
				to_block,
				filter.block_hash,
				address,
				topics
		) {
			let mut output = vec![];
			for log in logs {
				let address = log.0;
				let topics = log.1;
				let data = log.2;
				let block_hash = log.3;
				let block_number = log.4;
				let transaction_hash = log.5;
				let transaction_index = log.6;
				let log_index = log.7;
				let transaction_log_index = log.8;
				output.push(Log {
					address,
					topics,
					data: Bytes(data),
					block_hash,
					block_number,
					transaction_hash,
					transaction_index,
					log_index,
					transaction_log_index,
					removed: false
				});
			}
			return Ok(output);
		}
		Ok(vec![])
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

	fn is_listening(&self) -> Result<bool> {
		Ok(true)
	}
	fn version(&self) -> Result<String> {
		Ok(self.chain_id().unwrap().unwrap().to_string())
	}
}
