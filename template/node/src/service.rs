//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use std::{cell::RefCell, path::Path, sync::Arc, time::Duration};

use futures::{channel::mpsc, prelude::*};
// Substrate
use prometheus_endpoint::Registry;
use sc_client_api::{Backend as BackendT, BlockBackend};
use sc_consensus::{BasicQueue, BoxBlockImport};
use sc_consensus_grandpa::BlockNumberOps;
use sc_executor::HostFunctions as HostFunctionsT;
use sc_network_sync::strategy::warp::{WarpSyncConfig, WarpSyncProvider};
use sc_service::{error::Error as ServiceError, Configuration, PartialComponents, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker};
use sc_transaction_pool::FullPool;
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_api::ConstructRuntimeApi;
use sp_consensus_aura::sr25519::{AuthorityId as AuraId, AuthorityPair as AuraPair};
use sp_core::{H256, U256};
use sp_runtime::traits::{Block as BlockT, NumberFor};
// Runtime
use frontier_template_runtime::{
	opaque::Block, AccountId, Balance, Nonce, RuntimeApi, TransactionConverter,
};

pub use crate::eth::{db_config_dir, EthConfiguration};
use crate::{
	cli::Sealing,
	client::{BaseRuntimeApiCollection, FullBackend, FullClient, RuntimeApiCollection},
	eth::{
		new_frontier_partial, spawn_frontier_tasks, BackendType, EthCompatRuntimeApiCollection,
		FrontierBackend, FrontierBlockImport, FrontierPartialComponents, StorageOverride,
		StorageOverrideHandler,
	},
};

/// Only enable the benchmarking host functions when we actually want to benchmark.
#[cfg(feature = "runtime-benchmarks")]
pub type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
);
/// Otherwise we use empty host functions for ext host functions.
#[cfg(not(feature = "runtime-benchmarks"))]
pub type HostFunctions = sp_io::SubstrateHostFunctions;

pub type Backend = FullBackend<Block>;
pub type Client = FullClient<Block, RuntimeApi, HostFunctions>;

type FullSelectChain<B> = sc_consensus::LongestChain<FullBackend<B>, B>;
type GrandpaBlockImport<B, C> =
	sc_consensus_grandpa::GrandpaBlockImport<FullBackend<B>, B, C, FullSelectChain<B>>;
type GrandpaLinkHalf<B, C> = sc_consensus_grandpa::LinkHalf<B, C, FullSelectChain<B>>;

/// The minimum period of blocks on which justifications will be
/// imported and generated.
const GRANDPA_JUSTIFICATION_PERIOD: u32 = 512;

pub fn new_partial<B, RA, HF, BIQ>(
	config: &Configuration,
	eth_config: &EthConfiguration,
	build_import_queue: BIQ,
) -> Result<
	PartialComponents<
		FullClient<B, RA, HF>,
		FullBackend<B>,
		FullSelectChain<B>,
		BasicQueue<B>,
		FullPool<B, FullClient<B, RA, HF>>,
		(
			Option<Telemetry>,
			BoxBlockImport<B>,
			GrandpaLinkHalf<B, FullClient<B, RA, HF>>,
			FrontierBackend<B, FullClient<B, RA, HF>>,
			Arc<dyn StorageOverride<B>>,
		),
	>,
	ServiceError,
>
where
	B: BlockT<Hash = H256>,
	RA: ConstructRuntimeApi<B, FullClient<B, RA, HF>>,
	RA: Send + Sync + 'static,
	RA::RuntimeApi: BaseRuntimeApiCollection<B> + EthCompatRuntimeApiCollection<B>,
	HF: HostFunctionsT + 'static,
	BIQ: FnOnce(
		Arc<FullClient<B, RA, HF>>,
		&Configuration,
		&EthConfiguration,
		&TaskManager,
		Option<TelemetryHandle>,
		GrandpaBlockImport<B, FullClient<B, RA, HF>>,
	) -> Result<(BasicQueue<B>, BoxBlockImport<B>), ServiceError>,
{
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor = sc_service::new_wasm_executor(&config.executor);

	let (client, backend, keystore_container, task_manager) = sc_service::new_full_parts::<B, RA, _>(
		config,
		telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
		executor,
	)?;
	let client = Arc::new(client);

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager
			.spawn_handle()
			.spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());
	let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
		client.clone(),
		GRANDPA_JUSTIFICATION_PERIOD,
		&client,
		select_chain.clone(),
		telemetry.as_ref().map(|x| x.handle()),
	)?;

	let storage_override = Arc::new(StorageOverrideHandler::<B, _, _>::new(client.clone()));
	let frontier_backend = match eth_config.frontier_backend_type {
		BackendType::KeyValue => FrontierBackend::KeyValue(Arc::new(fc_db::kv::Backend::open(
			Arc::clone(&client),
			&config.database,
			&db_config_dir(config),
		)?)),
		BackendType::Sql => {
			let db_path = db_config_dir(config).join("sql");
			std::fs::create_dir_all(&db_path).expect("failed creating sql db directory");
			let backend = futures::executor::block_on(fc_db::sql::Backend::new(
				fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
					path: Path::new("sqlite:///")
						.join(db_path)
						.join("frontier.db3")
						.to_str()
						.unwrap(),
					create_if_missing: true,
					thread_count: eth_config.frontier_sql_backend_thread_count,
					cache_size: eth_config.frontier_sql_backend_cache_size,
				}),
				eth_config.frontier_sql_backend_pool_size,
				std::num::NonZeroU32::new(eth_config.frontier_sql_backend_num_ops_timeout),
				storage_override.clone(),
			))
			.unwrap_or_else(|err| panic!("failed creating sql backend: {:?}", err));
			FrontierBackend::Sql(Arc::new(backend))
		}
	};

	let (import_queue, block_import) = build_import_queue(
		client.clone(),
		config,
		eth_config,
		&task_manager,
		telemetry.as_ref().map(|x| x.handle()),
		grandpa_block_import,
	)?;

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	Ok(PartialComponents {
		client,
		backend,
		keystore_container,
		task_manager,
		select_chain,
		import_queue,
		transaction_pool,
		other: (
			telemetry,
			block_import,
			grandpa_link,
			frontier_backend,
			storage_override,
		),
	})
}

/// Build the import queue for the template runtime (aura + grandpa).
pub fn build_aura_grandpa_import_queue<B, RA, HF>(
	client: Arc<FullClient<B, RA, HF>>,
	config: &Configuration,
	eth_config: &EthConfiguration,
	task_manager: &TaskManager,
	telemetry: Option<TelemetryHandle>,
	grandpa_block_import: GrandpaBlockImport<B, FullClient<B, RA, HF>>,
) -> Result<(BasicQueue<B>, BoxBlockImport<B>), ServiceError>
where
	B: BlockT,
	NumberFor<B>: BlockNumberOps,
	RA: ConstructRuntimeApi<B, FullClient<B, RA, HF>>,
	RA: Send + Sync + 'static,
	RA::RuntimeApi: RuntimeApiCollection<B, AuraId, AccountId, Nonce, Balance>,
	HF: HostFunctionsT + 'static,
{
	let frontier_block_import =
		FrontierBlockImport::new(grandpa_block_import.clone(), client.clone());

	let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
	let target_gas_price = eth_config.target_gas_price;
	let create_inherent_data_providers = move |_, ()| async move {
		let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
		let slot =
			sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
				*timestamp,
				slot_duration,
			);
		let dynamic_fee = fp_dynamic_fee::InherentDataProvider(U256::from(target_gas_price));
		Ok((slot, timestamp, dynamic_fee))
	};

	let import_queue = sc_consensus_aura::import_queue::<AuraPair, _, _, _, _, _>(
		sc_consensus_aura::ImportQueueParams {
			block_import: frontier_block_import.clone(),
			justification_import: Some(Box::new(grandpa_block_import)),
			client,
			create_inherent_data_providers,
			spawner: &task_manager.spawn_essential_handle(),
			registry: config.prometheus_registry(),
			check_for_equivocation: Default::default(),
			telemetry,
			compatibility_mode: sc_consensus_aura::CompatibilityMode::None,
		},
	)
	.map_err::<ServiceError, _>(Into::into)?;

	Ok((import_queue, Box::new(frontier_block_import)))
}

/// Build the import queue for the template runtime (manual seal).
pub fn build_manual_seal_import_queue<B, RA, HF>(
	client: Arc<FullClient<B, RA, HF>>,
	config: &Configuration,
	_eth_config: &EthConfiguration,
	task_manager: &TaskManager,
	_telemetry: Option<TelemetryHandle>,
	_grandpa_block_import: GrandpaBlockImport<B, FullClient<B, RA, HF>>,
) -> Result<(BasicQueue<B>, BoxBlockImport<B>), ServiceError>
where
	B: BlockT,
	RA: ConstructRuntimeApi<B, FullClient<B, RA, HF>>,
	RA: Send + Sync + 'static,
	RA::RuntimeApi: RuntimeApiCollection<B, AuraId, AccountId, Nonce, Balance>,
	HF: HostFunctionsT + 'static,
{
	let frontier_block_import = FrontierBlockImport::new(client.clone(), client);
	Ok((
		sc_consensus_manual_seal::import_queue(
			Box::new(frontier_block_import.clone()),
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		),
		Box::new(frontier_block_import),
	))
}

/// Builds a new service for a full client.
pub async fn new_full<B, RA, HF, NB>(
	mut config: Configuration,
	eth_config: EthConfiguration,
	sealing: Option<Sealing>,
) -> Result<TaskManager, ServiceError>
where
	B: BlockT<Hash = H256>,
	NumberFor<B>: BlockNumberOps,
	<B as BlockT>::Header: Unpin,
	RA: ConstructRuntimeApi<B, FullClient<B, RA, HF>>,
	RA: Send + Sync + 'static,
	RA::RuntimeApi: RuntimeApiCollection<B, AuraId, AccountId, Nonce, Balance>,
	HF: HostFunctionsT + 'static,
	NB: sc_network::NetworkBackend<B, <B as BlockT>::Hash>,
{
	let build_import_queue = if sealing.is_some() {
		build_manual_seal_import_queue::<B, RA, HF>
	} else {
		build_aura_grandpa_import_queue::<B, RA, HF>
	};

	let PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (mut telemetry, block_import, grandpa_link, frontier_backend, storage_override),
	} = new_partial(&config, &eth_config, build_import_queue)?;

	let FrontierPartialComponents {
		filter_pool,
		fee_history_cache,
		fee_history_cache_limit,
	} = new_frontier_partial(&eth_config)?;

	let maybe_registry = config.prometheus_config.as_ref().map(|cfg| &cfg.registry);
	let mut net_config = sc_network::config::FullNetworkConfiguration::<_, _, NB>::new(
		&config.network,
		maybe_registry.cloned(),
	);
	let peer_store_handle = net_config.peer_store_handle();
	let metrics = NB::register_notification_metrics(maybe_registry);

	let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
		&client
			.block_hash(0u32.into())?
			.expect("Genesis block exists; qed"),
		&config.chain_spec,
	);

	let (grandpa_protocol_config, grandpa_notification_service) =
		sc_consensus_grandpa::grandpa_peers_set_config::<_, NB>(
			grandpa_protocol_name.clone(),
			metrics.clone(),
			peer_store_handle,
		);

	let warp_sync_config = if sealing.is_some() {
		None
	} else {
		net_config.add_notification_protocol(grandpa_protocol_config);
		let warp_sync: Arc<dyn WarpSyncProvider<B>> =
			Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
				backend.clone(),
				grandpa_link.shared_authority_set().clone(),
				Vec::new(),
			));
		Some(WarpSyncConfig::WithProvider(warp_sync))
	};

	let (network, system_rpc_tx, tx_handler_controller, network_starter, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_config,
			block_relay: None,
			metrics,
		})?;

	if config.offchain_worker.enabled {
		task_manager.spawn_handle().spawn(
			"offchain-workers-runner",
			"offchain-worker",
			sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
				runtime_api_provider: client.clone(),
				is_validator: config.role.is_authority(),
				keystore: Some(keystore_container.keystore()),
				offchain_db: backend.offchain_storage(),
				transaction_pool: Some(OffchainTransactionPoolFactory::new(
					transaction_pool.clone(),
				)),
				network_provider: Arc::new(network.clone()),
				enable_http_requests: true,
				custom_extensions: |_| vec![],
			})
			.run(client.clone(), task_manager.spawn_handle())
			.boxed(),
		);
	}

	let role = config.role.clone();
	let force_authoring = config.force_authoring;
	let name = config.network.node_name.clone();
	let frontier_backend = Arc::new(frontier_backend);
	let enable_grandpa = !config.disable_grandpa && sealing.is_none();
	let prometheus_registry = config.prometheus_registry().cloned();

	// Channel for the rpc handler to communicate with the authorship task.
	let (command_sink, commands_stream) = mpsc::channel(1000);

	// Sinks for pubsub notifications.
	// Everytime a new subscription is created, a new mpsc channel is added to the sink pool.
	// The MappingSyncWorker sends through the channel on block import and the subscription emits a notification to the subscriber on receiving a message through this channel.
	// This way we avoid race conditions when using native substrate block import notification stream.
	let pubsub_notification_sinks: fc_mapping_sync::EthereumBlockNotificationSinks<
		fc_mapping_sync::EthereumBlockNotification<B>,
	> = Default::default();
	let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

	// for ethereum-compatibility rpc.
	config.rpc.id_provider = Some(Box::new(fc_rpc::EthereumSubIdProvider));

	let rpc_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		let network = network.clone();
		let sync_service = sync_service.clone();

		let is_authority = role.is_authority();
		let enable_dev_signer = eth_config.enable_dev_signer;
		let max_past_logs = eth_config.max_past_logs;
		let execute_gas_limit_multiplier = eth_config.execute_gas_limit_multiplier;
		let filter_pool = filter_pool.clone();
		let frontier_backend = frontier_backend.clone();
		let pubsub_notification_sinks = pubsub_notification_sinks.clone();
		let storage_override = storage_override.clone();
		let fee_history_cache = fee_history_cache.clone();
		let block_data_cache = Arc::new(fc_rpc::EthBlockDataCacheTask::new(
			task_manager.spawn_handle(),
			storage_override.clone(),
			eth_config.eth_log_block_cache,
			eth_config.eth_statuses_cache,
			prometheus_registry.clone(),
		));

		let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
		let target_gas_price = eth_config.target_gas_price;
		let pending_create_inherent_data_providers = move |_, ()| async move {
			let current = sp_timestamp::InherentDataProvider::from_system_time();
			let next_slot = current.timestamp().as_millis() + slot_duration.as_millis();
			let timestamp = sp_timestamp::InherentDataProvider::new(next_slot.into());
			let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
				*timestamp,
				slot_duration,
			);
			let dynamic_fee = fp_dynamic_fee::InherentDataProvider(U256::from(target_gas_price));
			Ok((slot, timestamp, dynamic_fee))
		};

		Box::new(move |subscription_task_executor| {
			let eth_deps = crate::rpc::EthDeps {
				client: client.clone(),
				pool: pool.clone(),
				graph: pool.pool().clone(),
				converter: Some(TransactionConverter::<B>::default()),
				is_authority,
				enable_dev_signer,
				network: network.clone(),
				sync: sync_service.clone(),
				frontier_backend: match &*frontier_backend {
					fc_db::Backend::KeyValue(b) => b.clone(),
					fc_db::Backend::Sql(b) => b.clone(),
				},
				storage_override: storage_override.clone(),
				block_data_cache: block_data_cache.clone(),
				filter_pool: filter_pool.clone(),
				max_past_logs,
				fee_history_cache: fee_history_cache.clone(),
				fee_history_cache_limit,
				execute_gas_limit_multiplier,
				forced_parent_hashes: None,
				pending_create_inherent_data_providers,
			};
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				command_sink: if sealing.is_some() {
					Some(command_sink.clone())
				} else {
					None
				},
				eth: eth_deps,
			};
			crate::rpc::create_full(
				deps,
				subscription_task_executor,
				pubsub_notification_sinks.clone(),
			)
			.map_err(Into::into)
		})
	};

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		config,
		client: client.clone(),
		backend: backend.clone(),
		task_manager: &mut task_manager,
		keystore: keystore_container.keystore(),
		transaction_pool: transaction_pool.clone(),
		rpc_builder,
		network: network.clone(),
		system_rpc_tx,
		tx_handler_controller,
		sync_service: sync_service.clone(),
		telemetry: telemetry.as_mut(),
	})?;

	spawn_frontier_tasks(
		&task_manager,
		client.clone(),
		backend,
		frontier_backend,
		filter_pool,
		storage_override,
		fee_history_cache,
		fee_history_cache_limit,
		sync_service.clone(),
		pubsub_notification_sinks,
	)
	.await;

	if role.is_authority() {
		// manual-seal authorship
		if let Some(sealing) = sealing {
			run_manual_seal_authorship(
				&eth_config,
				sealing,
				client,
				transaction_pool,
				select_chain,
				block_import,
				&task_manager,
				prometheus_registry.as_ref(),
				telemetry.as_ref(),
				commands_stream,
			)?;

			network_starter.start_network();
			log::info!("Manual Seal Ready");
			return Ok(task_manager);
		}

		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
		let target_gas_price = eth_config.target_gas_price;
		let create_inherent_data_providers = move |_, ()| async move {
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
			let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
				*timestamp,
				slot_duration,
			);
			let dynamic_fee = fp_dynamic_fee::InherentDataProvider(U256::from(target_gas_price));
			Ok((slot, timestamp, dynamic_fee))
		};

		let aura = sc_consensus_aura::start_aura::<AuraPair, _, _, _, _, _, _, _, _, _, _>(
			sc_consensus_aura::StartAuraParams {
				slot_duration,
				client,
				select_chain,
				block_import,
				proposer_factory,
				sync_oracle: sync_service.clone(),
				justification_sync_link: sync_service.clone(),
				create_inherent_data_providers,
				force_authoring,
				backoff_authoring_blocks: Option::<()>::None,
				keystore: keystore_container.keystore(),
				block_proposal_slot_portion: sc_consensus_aura::SlotProportion::new(2f32 / 3f32),
				max_block_proposal_slot_portion: None,
				telemetry: telemetry.as_ref().map(|x| x.handle()),
				compatibility_mode: sc_consensus_aura::CompatibilityMode::None,
			},
		)?;
		// the AURA authoring task is considered essential, i.e. if it
		// fails we take down the service with it.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("aura", Some("block-authoring"), aura);
	}

	if enable_grandpa {
		// if the node isn't actively participating in consensus then it doesn't
		// need a keystore, regardless of which protocol we use below.
		let keystore = if role.is_authority() {
			Some(keystore_container.keystore())
		} else {
			None
		};

		let grandpa_config = sc_consensus_grandpa::Config {
			// FIXME #1578 make this available through chainspec
			gossip_duration: Duration::from_millis(333),
			justification_generation_period: GRANDPA_JUSTIFICATION_PERIOD,
			name: Some(name),
			observer_enabled: false,
			keystore,
			local_role: role,
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			protocol_name: grandpa_protocol_name,
		};

		// start the full GRANDPA voter
		// NOTE: non-authorities could run the GRANDPA observer protocol, but at
		// this point the full voter should provide better guarantees of block
		// and vote data availability than the observer. The observer has not
		// been tested extensively yet and having most nodes in a network run it
		// could lead to finality stalls.
		let grandpa_voter =
			sc_consensus_grandpa::run_grandpa_voter(sc_consensus_grandpa::GrandpaParams {
				config: grandpa_config,
				link: grandpa_link,
				network,
				sync: sync_service,
				notification_service: grandpa_notification_service,
				voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
				prometheus_registry,
				shared_voter_state: sc_consensus_grandpa::SharedVoterState::empty(),
				telemetry: telemetry.as_ref().map(|x| x.handle()),
				offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(transaction_pool),
			})?;

		// the GRANDPA voter task is considered infallible, i.e.
		// if it fails we take down the service with it.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("grandpa-voter", None, grandpa_voter);
	}

	network_starter.start_network();
	Ok(task_manager)
}

fn run_manual_seal_authorship<B, RA, HF>(
	eth_config: &EthConfiguration,
	sealing: Sealing,
	client: Arc<FullClient<B, RA, HF>>,
	transaction_pool: Arc<FullPool<B, FullClient<B, RA, HF>>>,
	select_chain: FullSelectChain<B>,
	block_import: BoxBlockImport<B>,
	task_manager: &TaskManager,
	prometheus_registry: Option<&Registry>,
	telemetry: Option<&Telemetry>,
	commands_stream: mpsc::Receiver<
		sc_consensus_manual_seal::rpc::EngineCommand<<B as BlockT>::Hash>,
	>,
) -> Result<(), ServiceError>
where
	B: BlockT,
	RA: ConstructRuntimeApi<B, FullClient<B, RA, HF>>,
	RA: Send + Sync + 'static,
	RA::RuntimeApi: RuntimeApiCollection<B, AuraId, AccountId, Nonce, Balance>,
	HF: HostFunctionsT + 'static,
{
	let proposer_factory = sc_basic_authorship::ProposerFactory::new(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool.clone(),
		prometheus_registry,
		telemetry.as_ref().map(|x| x.handle()),
	);

	thread_local!(static TIMESTAMP: RefCell<u64> = const { RefCell::new(0) });

	/// Provide a mock duration starting at 0 in millisecond for timestamp inherent.
	/// Each call will increment timestamp by slot_duration making Aura think time has passed.
	struct MockTimestampInherentDataProvider;

	#[async_trait::async_trait]
	impl sp_inherents::InherentDataProvider for MockTimestampInherentDataProvider {
		async fn provide_inherent_data(
			&self,
			inherent_data: &mut sp_inherents::InherentData,
		) -> Result<(), sp_inherents::Error> {
			TIMESTAMP.with(|x| {
				*x.borrow_mut() += frontier_template_runtime::SLOT_DURATION;
				inherent_data.put_data(sp_timestamp::INHERENT_IDENTIFIER, &*x.borrow())
			})
		}

		async fn try_handle_error(
			&self,
			_identifier: &sp_inherents::InherentIdentifier,
			_error: &[u8],
		) -> Option<Result<(), sp_inherents::Error>> {
			// The pallet never reports error.
			None
		}
	}

	let target_gas_price = eth_config.target_gas_price;
	let create_inherent_data_providers = move |_, ()| async move {
		let timestamp = MockTimestampInherentDataProvider;
		let dynamic_fee = fp_dynamic_fee::InherentDataProvider(U256::from(target_gas_price));
		Ok((timestamp, dynamic_fee))
	};

	let manual_seal = match sealing {
		Sealing::Manual => future::Either::Left(sc_consensus_manual_seal::run_manual_seal(
			sc_consensus_manual_seal::ManualSealParams {
				block_import,
				env: proposer_factory,
				client,
				pool: transaction_pool,
				commands_stream,
				select_chain,
				consensus_data_provider: None,
				create_inherent_data_providers,
			},
		)),
		Sealing::Instant => future::Either::Right(sc_consensus_manual_seal::run_instant_seal(
			sc_consensus_manual_seal::InstantSealParams {
				block_import,
				env: proposer_factory,
				client,
				pool: transaction_pool,
				select_chain,
				consensus_data_provider: None,
				create_inherent_data_providers,
			},
		)),
	};

	// we spawn the future on a background thread managed by service.
	task_manager
		.spawn_essential_handle()
		.spawn_blocking("manual-seal", None, manual_seal);
	Ok(())
}

pub async fn build_full(
	config: Configuration,
	eth_config: EthConfiguration,
	sealing: Option<Sealing>,
) -> Result<TaskManager, ServiceError> {
	new_full::<Block, RuntimeApi, HostFunctions, sc_network::NetworkWorker<_, _>>(
		config, eth_config, sealing,
	)
	.await
}

pub fn new_chain_ops(
	config: &mut Configuration,
	eth_config: &EthConfiguration,
) -> Result<
	(
		Arc<Client>,
		Arc<Backend>,
		BasicQueue<Block>,
		TaskManager,
		FrontierBackend<Block, Client>,
	),
	ServiceError,
> {
	config.keystore = sc_service::config::KeystoreConfig::InMemory;
	let PartialComponents {
		client,
		backend,
		import_queue,
		task_manager,
		other,
		..
	} = new_partial::<Block, RuntimeApi, HostFunctions, _>(
		config,
		eth_config,
		build_aura_grandpa_import_queue,
	)?;
	Ok((client, backend, import_queue, task_manager, other.3))
}
