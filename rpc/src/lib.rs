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
use ethereum_types::{H160, H256, H64, U256, U64};
use jsonrpc_core::{BoxFuture, Result, ErrorCode, Error, futures::future::{self, Future}};
use futures::future::TryFutureExt;
use sp_runtime::traits::{Block as BlockT, Header as _};
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
	SyncStatus, Transaction, Work,
};
use frontier_rpc_primitives::{EthereumRuntimeApi, ConvertTransaction};

pub use frontier_rpc_core::EthApiServer;

fn internal_err(message: &str) -> Error {
	Error {
		code: ErrorCode::InternalError,
		message: message.to_string(),
		data: None
	}
}

pub struct EthApi<B: BlockT, C, SC, P, CT, BE> {
	pool: Arc<P>,
	client: Arc<C>,
	select_chain: SC,
	convert_transaction: CT,
	_marker: PhantomData<(B,BE)>,
}

impl<B: BlockT, C, SC, P, CT, BE> EthApi<B, C, SC, P, CT, BE> {
	pub fn new(
		client: Arc<C>,
		select_chain: SC,
		pool: Arc<P>,
		convert_transaction: CT,
	) -> Self {
		Self { client, select_chain, pool, convert_transaction, _marker: PhantomData }
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
		Ok(false)
	}

	fn chain_id(&self) -> Result<Option<U64>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		Ok(Some(self.client.runtime_api().chain_id(&BlockId::Hash(header.hash()))
				.map_err(|_| internal_err("fetch runtime chain id failed"))?.into()))
	}

	fn gas_price(&self) -> BoxFuture<U256> {
		unimplemented!("gas_price");
	}

	fn accounts(&self) -> Result<Vec<H160>> {
		Ok(vec![])
	}

	fn block_number(&self) -> Result<U256> {
		unimplemented!("block_number");
	}

	fn balance(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("balance");
	}

	fn proof(&self, _: H160, _: Vec<H256>, _: Option<BlockNumber>) -> BoxFuture<EthAccount> {
		unimplemented!("proof");
	}

	fn storage_at(&self, _: H160, _: U256, _: Option<BlockNumber>) -> BoxFuture<H256> {
		unimplemented!("storage_at");
	}

	fn block_by_hash(&self, _: H256, _: bool) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("block_by_hash");
	}

	fn block_by_number(&self, _: BlockNumber, _: bool) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("block_by_number");
	}

	fn transaction_count(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		if let Some(number) = number {
			if number != BlockNumber::Latest {
				unimplemented!("fetch nonce for past blocks is not yet supported");
			}
		}

		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		Ok(self.client.runtime_api().account_basic(&BlockId::Hash(header.hash()), address)
		   .map_err(|_| internal_err("fetch runtime account basic failed"))?.nonce.into())
	}

	fn block_transaction_count_by_hash(&self, _: H256) -> BoxFuture<Option<U256>> {
		unimplemented!("block_transaction_count_by_hash");
	}

	fn block_transaction_count_by_number(&self, _: BlockNumber) -> BoxFuture<Option<U256>> {
		unimplemented!("block_transaction_count_by_number");
	}

	fn block_uncles_count_by_hash(&self, _: H256) -> Result<U256> {
		Ok(U256::zero())
	}

	fn block_uncles_count_by_number(&self, _: BlockNumber) -> Result<U256> {
		Ok(U256::zero())
	}

	fn code_at(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<Bytes> {
		unimplemented!("code_at");
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

	fn call(&self, _: CallRequest, _: Option<BlockNumber>) -> BoxFuture<Bytes> {
		unimplemented!("call");
	}

	fn estimate_gas(&self, _: CallRequest, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("estimate_gas");
	}

	fn transaction_by_hash(&self, _: H256) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_hash");
	}

	fn transaction_by_block_hash_and_index(
		&self,
		_: H256,
		_: Index,
	) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_block_hash_and_index");
	}

	fn transaction_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_block_number_and_index");
	}

	fn transaction_receipt(&self, hash: H256) -> Result<Option<Receipt>> {
		let header = self.select_chain.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;
		let status = self.client.runtime_api()
			.transaction_status(&BlockId::Hash(header.hash()), hash)
			.map_err(|_| internal_err("fetch runtime transaction status failed"))?;
		let receipt = status.map(|status| {
			Receipt {
				transaction_hash: Some(status.transaction_hash),
				transaction_index: Some(status.transaction_index.into()),
				block_hash: Some(Default::default()),
				from: Some(status.from),
				to: status.to,
				block_number: Some(Default::default()),
				cumulative_gas_used: Default::default(),
				gas_used: Some(Default::default()),
				contract_address: status.contract_address,
				logs: Vec::new(),
				state_root: None,
				logs_bloom: Default::default(),
				status_code: None,
			}
		});

		Ok(receipt)
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
		unimplemented!("compilers");
	}

	fn compile_lll(&self, _: String) -> Result<Bytes> {
		unimplemented!("compile_lll");
	}

	fn compile_solidity(&self, _: String) -> Result<Bytes> {
		unimplemented!("compile_solidity");
	}

	fn compile_serpent(&self, _: String) -> Result<Bytes> {
		unimplemented!("compile_serpent");
	}

	fn logs(&self, _: Filter) -> BoxFuture<Vec<Log>> {
		unimplemented!("logs");
	}

	fn work(&self, _: Option<u64>) -> Result<Work> {
		unimplemented!("work");
	}

	fn submit_work(&self, _: H64, _: H256, _: H256) -> Result<bool> {
		unimplemented!("submit_work");
	}

	fn submit_hashrate(&self, _: U256, _: H256) -> Result<bool> {
		unimplemented!("submit_hashrate");
	}
}
