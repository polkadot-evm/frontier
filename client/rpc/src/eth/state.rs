use std::{marker::PhantomData, sync::Arc};

use ethereum_types::{H160, H256, U256};
use jsonrpc_core::Result;

use codec::Encode;
use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sc_transaction_pool::{ChainApi, Pool};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT},
};

use fc_rpc_core::{types::*, EthStateApi as EthStateApiT};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	eth::pending_runtime_api, frontier_backend_client, internal_err, overrides::OverrideHandle,
};

pub struct EthStateApi<B: BlockT, C, BE, P, A: ChainApi> {
	client: Arc<C>,
	overrides: Arc<OverrideHandle<B>>,
	backend: Arc<fc_db::Backend<B>>,
	pool: Arc<P>,
	graph: Arc<Pool<A>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, C, BE, P, A: ChainApi> EthStateApi<B, C, BE, P, A> {
	pub fn new(
		client: Arc<C>,
		overrides: Arc<OverrideHandle<B>>,
		backend: Arc<fc_db::Backend<B>>,
		pool: Arc<P>,
		graph: Arc<Pool<A>>,
	) -> Self {
		Self {
			client,
			overrides,
			backend,
			pool,
			graph,
			_marker: PhantomData,
		}
	}
}

impl<B, C, BE, P, A> EthStateApiT for EthStateApi<B, C, BE, P, A>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	P: TransactionPool<Block = B> + Send + Sync + 'static,
	A: ChainApi<Block = B> + 'static,
{
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
}
