use std::{marker::PhantomData, sync::Arc};

use ethereum_types::{H160, H256, U256, U64};
use jsonrpc_core::Result;

use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sc_network::{ExHashT, NetworkService};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT, UniqueSaturatedInto},
};

use fc_rpc_core::{types::*, EthClientApi as EthClientApiT};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{frontier_backend_client, internal_err, overrides::OverrideHandle, EthSigner};

pub struct EthClientApi<B: BlockT, C, BE, H: ExHashT> {
	client: Arc<C>,
	overrides: Arc<OverrideHandle<B>>,
	network: Arc<NetworkService<B, H>>,
	signers: Arc<Vec<Box<dyn EthSigner>>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, C, BE, H: ExHashT> EthClientApi<B, C, BE, H> {
	pub fn new(
		client: Arc<C>,
		overrides: Arc<OverrideHandle<B>>,
		network: Arc<NetworkService<B, H>>,
		signers: Arc<Vec<Box<dyn EthSigner>>>,
	) -> Self {
		Self {
			client,
			overrides,
			network,
			signers,
			_marker: PhantomData,
		}
	}
}

impl<B, C, BE, H: ExHashT> EthClientApiT for EthClientApi<B, C, BE, H>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
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

	fn accounts(&self) -> Result<Vec<H160>> {
		let mut accounts = Vec::new();
		for signer in &*self.signers {
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
}
