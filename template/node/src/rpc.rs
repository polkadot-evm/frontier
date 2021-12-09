//! A collection of node-specific RPC methods.

use std::sync::Arc;

use fc_rpc::{
	EthBlockDataCache, OverrideHandle, RuntimeApiStorageOverride, SchemaV1Override,
	SchemaV2Override, StorageOverride,
};
use fc_rpc_core::types::FilterPool;
use frontier_template_runtime::{opaque::Block, AccountId, Balance, Hash, Index};
use jsonrpsee::RpcModule;
use pallet_ethereum::EthereumStorageSchema;
use sc_client_api::{
	backend::{AuxStore, Backend, StateBackend, StorageProvider},
	client::BlockchainEvents,
};
use sc_consensus_manual_seal::rpc::{ManualSeal, ManualSealApiServer};
use sc_network::NetworkService;
use sc_rpc::SubscriptionTaskExecutor;
use sc_rpc_api::DenyUnsafe;
use sc_service::TransactionPool;
use sc_transaction_pool::{ChainApi, Pool};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::BlakeTwo256;
use std::collections::BTreeMap;

/// Full client dependencies.
pub struct FullDeps<C, P, A: ChainApi> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// Graph pool instance.
	pub graph: Arc<Pool<A>>,
	/// Whether to deny unsafe calls
	pub deny_unsafe: DenyUnsafe,
	/// The Node authority flag
	pub is_authority: bool,
	/// Whether to enable dev signer
	pub enable_dev_signer: bool,
	/// Network service
	pub network: Arc<NetworkService<Block, Hash>>,
	/// EthFilterApi pool.
	pub filter_pool: Option<FilterPool>,
	/// Backend.
	pub backend: Arc<fc_db::Backend<Block>>,
	/// Maximum number of logs in a query.
	pub max_past_logs: u32,
	/// Manual seal command sink
	pub command_sink:
		Option<futures::channel::mpsc::Sender<sc_consensus_manual_seal::rpc::EngineCommand<Hash>>>,
}

/// Instantiate all Full RPC extensions.
pub fn create_full<C, P, BE, A>(
	deps: FullDeps<C, P, A>,
	subscription_task_executor: SubscriptionTaskExecutor,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
	BE: Backend<Block> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	C: ProvideRuntimeApi<Block> + StorageProvider<Block, BE> + AuxStore,
	C: BlockchainEvents<Block>,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError>,
	C: Send + Sync + 'static,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Index>,
	C::Api: BlockBuilder<Block>,
	C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
	C::Api: fp_rpc::EthereumRuntimeRPCApi<Block>,
	P: TransactionPool<Block = Block> + 'static,
	A: ChainApi<Block = Block> + 'static,
{
	use fc_rpc::{
		EthApi, EthApiServer, EthDevSigner, EthFilterApi, EthFilterApiServer, EthPubSubApi,
		EthPubSubApiServer, EthSigner, NetApi, NetApiServer, Web3Api, Web3ApiServer,
	};
	use pallet_transaction_payment_rpc::{TransactionPaymentApiServer, TransactionPaymentRpc};
	use substrate_frame_rpc_system::{SystemApiServer, SystemRpc};

	let mut io = RpcModule::new(());
	let FullDeps {
		client,
		pool,
		graph,
		deny_unsafe,
		is_authority,
		network,
		filter_pool,
		command_sink,
		backend,
		max_past_logs,
		enable_dev_signer,
	} = deps;

	io.merge(SystemRpc::new(client.clone(), pool.clone(), deny_unsafe).into_rpc())?;
	io.merge(TransactionPaymentRpc::new(client.clone()).into_rpc())?;

	let mut signers = Vec::new();
	if enable_dev_signer {
		signers.push(Box::new(EthDevSigner::new()) as Box<dyn EthSigner>);
	}
	let mut overrides_map = BTreeMap::new();
	overrides_map.insert(
		EthereumStorageSchema::V1,
		Box::new(SchemaV1Override::new(client.clone()))
			as Box<dyn StorageOverride<_> + Send + Sync>,
	);
	overrides_map.insert(
		EthereumStorageSchema::V2,
		Box::new(SchemaV2Override::new(client.clone()))
			as Box<dyn StorageOverride<_> + Send + Sync>,
	);

	let overrides = Arc::new(OverrideHandle {
		schemas: overrides_map,
		fallback: Box::new(RuntimeApiStorageOverride::new(client.clone())),
	});

	let block_data_cache = Arc::new(EthBlockDataCache::new(50, 50));

	io.merge(
		EthApi::new(
			client.clone(),
			pool.clone(),
			graph,
			frontier_template_runtime::TransactionConverter,
			network.clone(),
			signers,
			overrides.clone(),
			backend.clone(),
			is_authority,
			max_past_logs,
			block_data_cache.clone(),
		)
		.into_rpc(),
	)?;

	if let Some(filter_pool) = filter_pool {
		io.merge(
			EthFilterApi::new(
				client.clone(),
				backend,
				filter_pool.clone(),
				500 as usize, // max stored filters
				overrides.clone(),
				max_past_logs,
				block_data_cache.clone(),
			)
			.into_rpc(),
		)?;
	}

	io.merge(
		NetApi::new(
			client.clone(),
			network.clone(),
			// Whether to format the `peer_count` response as Hex (default) or not.
			true,
		)
		.into_rpc(),
	)?;

	io.merge(Web3Api::new(client.clone()).into_rpc())?;

	io.merge(
		EthPubSubApi::new(
			pool.clone(),
			client.clone(),
			network.clone(),
			subscription_task_executor,
			overrides,
		)
		.into_rpc(),
	)?;

	match command_sink {
		Some(command_sink) => {
			io.merge(
				// We provide the rpc handler with the sending end of the channel to allow the rpc
				// send EngineCommands to the background block authorship task.
				ManualSeal::new(command_sink).into_rpc(),
			)?;
		}
		_ => {}
	}

	Ok(io)
}
