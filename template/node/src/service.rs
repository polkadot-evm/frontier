//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use fc_consensus::FrontierBlockImport;
use fc_mapping_sync::{MappingSyncWorker, SyncStrategy};
use fc_rpc::EthTask;
use fc_rpc_core::types::{FeeHistoryCache, FilterPool};
use frontier_template_runtime::{self, opaque::Block, RuntimeApi, SLOT_DURATION};
use futures::StreamExt;
use sc_cli::SubstrateCli;
use sc_client_api::{BlockBackend, BlockchainEvents, ExecutorProvider};
use sc_consensus_aura::{ImportQueueParams, SlotProportion, StartAuraParams};
#[cfg(feature = "manual-seal")]
use sc_consensus_manual_seal::{self as manual_seal};
pub use sc_executor::NativeElseWasmExecutor;
use sc_finality_grandpa::SharedVoterState;
use sc_keystore::LocalKeystore;
use sc_network::warp_request_handler::WarpSyncProvider;
use sc_service::{error::Error as ServiceError, BasePath, Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sp_consensus::SlotData;
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use sp_core::U256;
use sp_inherents::{InherentData, InherentIdentifier};
use std::{
	cell::RefCell,
	collections::BTreeMap,
	sync::{Arc, Mutex},
	time::Duration,
};

use crate::cli::Cli;
#[cfg(feature = "manual-seal")]
use crate::cli::Sealing;

// Our native executor instance.
pub struct ExecutorDispatch;

impl sc_executor::NativeExecutionDispatch for ExecutorDispatch {
	/// Only enable the benchmarking host functions when we actually want to benchmark.
	#[cfg(feature = "runtime-benchmarks")]
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	/// Otherwise we only use the default Substrate host functions.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type ExtendHostFunctions = ();

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		frontier_template_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		frontier_template_runtime::native_version()
	}
}

type FullClient =
	sc_service::TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

#[cfg(feature = "aura")]
pub type ConsensusResult = (
	FrontierBlockImport<
		Block,
		sc_finality_grandpa::GrandpaBlockImport<FullBackend, Block, FullClient, FullSelectChain>,
		FullClient,
	>,
	sc_finality_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
);

#[cfg(feature = "manual-seal")]
pub type ConsensusResult = (
	FrontierBlockImport<Block, Arc<FullClient>, FullClient>,
	Sealing,
);

/// Provide a mock duration starting at 0 in millisecond for timestamp inherent.
/// Each call will increment timestamp by slot_duration making Aura think time has passed.
pub struct MockTimestampInherentDataProvider;

pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"timstap0";

thread_local!(static TIMESTAMP: RefCell<u64> = RefCell::new(0));

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for MockTimestampInherentDataProvider {
	fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		TIMESTAMP.with(|x| {
			*x.borrow_mut() += SLOT_DURATION;
			inherent_data.put_data(INHERENT_IDENTIFIER, &*x.borrow())
		})
	}

	async fn try_handle_error(
		&self,
		_identifier: &InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		// The pallet never reports error.
		None
	}
}

pub fn frontier_database_dir(config: &Configuration) -> std::path::PathBuf {
	let config_dir = config
		.base_path
		.as_ref()
		.map(|base_path| base_path.config_dir(config.chain_spec.id()))
		.unwrap_or_else(|| {
			BasePath::from_project("", "", &crate::cli::Cli::executable_name())
				.config_dir(config.chain_spec.id())
		});
	config_dir.join("frontier").join("db")
}

pub fn open_frontier_backend(config: &Configuration) -> Result<Arc<fc_db::Backend<Block>>, String> {
	Ok(Arc::new(fc_db::Backend::<Block>::new(
		&fc_db::DatabaseSettings {
			source: fc_db::DatabaseSettingsSrc::RocksDb {
				path: frontier_database_dir(&config),
				cache_size: 0,
			},
		},
	)?))
}

pub fn new_partial(
	config: &Configuration,
	cli: &Cli,
) -> Result<
	sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block, FullClient>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(
			ConsensusResult,
			Option<FilterPool>,
			Arc<fc_db::Backend<Block>>,
			Option<Telemetry>,
			FeeHistoryCache,
		),
	>,
	ServiceError,
> {
	if config.keystore_remote.is_some() {
		return Err(ServiceError::Other(format!(
			"Remote Keystores are not supported."
		)));
	}

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

	let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
		config.wasm_method,
		config.default_heap_pages,
		config.max_runtime_instances,
		config.runtime_cache_size,
	);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			&config,
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

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let filter_pool: Option<FilterPool> = Some(Arc::new(Mutex::new(BTreeMap::new())));
	let fee_history_cache: FeeHistoryCache = Arc::new(Mutex::new(BTreeMap::new()));

	let frontier_backend = open_frontier_backend(config)?;

	#[cfg(feature = "manual-seal")]
	{
		let sealing = cli.run.sealing;

		let frontier_block_import =
			FrontierBlockImport::new(client.clone(), client.clone(), frontier_backend.clone());

		let import_queue = sc_consensus_manual_seal::import_queue(
			Box::new(frontier_block_import.clone()),
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		);

		Ok(sc_service::PartialComponents {
			client,
			backend,
			task_manager,
			import_queue,
			keystore_container,
			select_chain,
			transaction_pool,
			other: (
				(frontier_block_import, sealing),
				filter_pool,
				frontier_backend,
				telemetry,
				fee_history_cache,
			),
		})
	}

	#[cfg(feature = "aura")]
	{
		let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
			client.clone(),
			&(client.clone() as Arc<_>),
			select_chain.clone(),
			telemetry.as_ref().map(|x| x.handle()),
		)?;

		let frontier_block_import = FrontierBlockImport::new(
			grandpa_block_import.clone(),
			client.clone(),
			frontier_backend.clone(),
		);

		let slot_duration = sc_consensus_aura::slot_duration(&*client)?.slot_duration();
		let target_gas_price = cli.run.target_gas_price;

		let import_queue =
			sc_consensus_aura::import_queue::<AuraPair, _, _, _, _, _, _>(ImportQueueParams {
				block_import: frontier_block_import.clone(),
				justification_import: Some(Box::new(grandpa_block_import.clone())),
				client: client.clone(),
				create_inherent_data_providers: move |_, ()| async move {
					let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

					let slot =
						sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_duration(
							*timestamp,
							slot_duration,
						);

					let dynamic_fee =
						pallet_dynamic_fee::InherentDataProvider(U256::from(target_gas_price));

					Ok((timestamp, slot, dynamic_fee))
				},
				spawner: &task_manager.spawn_essential_handle(),
				can_author_with: sp_consensus::CanAuthorWithNativeVersion::new(
					client.executor().clone(),
				),
				registry: config.prometheus_registry(),
				check_for_equivocation: Default::default(),
				telemetry: telemetry.as_ref().map(|x| x.handle()),
			})?;

		Ok(sc_service::PartialComponents {
			client,
			backend,
			task_manager,
			import_queue,
			keystore_container,
			select_chain,
			transaction_pool,
			other: (
				(frontier_block_import, grandpa_link),
				filter_pool,
				frontier_backend,
				telemetry,
				fee_history_cache,
			),
		})
	}
}

fn remote_keystore(_url: &String) -> Result<Arc<LocalKeystore>, &'static str> {
	// FIXME: here would the concrete keystore be built,
	//        must return a concrete type (NOT `LocalKeystore`) that
	//        implements `CryptoStore` and `SyncCryptoStore`
	Err("Remote Keystore not supported.")
}

/// Builds a new service for a full client.
pub fn new_full(mut config: Configuration, cli: &Cli) -> Result<TaskManager, ServiceError> {
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		mut keystore_container,
		select_chain,
		transaction_pool,
		other: (consensus_result, filter_pool, frontier_backend, mut telemetry, fee_history_cache),
	} = new_partial(&config, &cli)?;

	if let Some(url) = &config.keystore_remote {
		match remote_keystore(url) {
			Ok(k) => keystore_container.set_remote_keystore(k),
			Err(e) => {
				return Err(ServiceError::Other(format!(
					"Error hooking up remote keystore for {}: {}",
					url, e
				)))
			}
		};
	}
	let grandpa_protocol_name = sc_finality_grandpa::protocol_standard_name(
		&client
			.block_hash(0)
			.ok()
			.flatten()
			.expect("Genesis block exists; qed"),
		&config.chain_spec,
	);

	let warp_sync: Option<Arc<dyn WarpSyncProvider<Block>>> = {
		#[cfg(feature = "aura")]
		{
			config
				.network
				.extra_sets
				.push(sc_finality_grandpa::grandpa_peers_set_config(
					grandpa_protocol_name.clone(),
				));
			Some(Arc::new(
				sc_finality_grandpa::warp_proof::NetworkProvider::new(
					backend.clone(),
					consensus_result.1.shared_authority_set().clone(),
					Vec::default(),
				),
			))
		}
		#[cfg(feature = "manual-seal")]
		{
			None
		}
	};

	let (network, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync,
		})?;

	// Channel for the rpc handler to communicate with the authorship task.
	let (command_sink, commands_stream) = futures::channel::mpsc::channel(1000);

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config,
			task_manager.spawn_handle(),
			client.clone(),
			network.clone(),
		);
	}

	let role = config.role.clone();
	let force_authoring = config.force_authoring;
	let backoff_authoring_blocks: Option<()> = None;
	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();
	let is_authority = config.role.is_authority();
	let enable_dev_signer = cli.run.enable_dev_signer;
	let subscription_task_executor =
		sc_rpc::SubscriptionTaskExecutor::new(task_manager.spawn_handle());
	let overrides = crate::rpc::overrides_handle(client.clone());
	let fee_history_limit = cli.run.fee_history_limit;

	let block_data_cache = Arc::new(fc_rpc::EthBlockDataCache::new(
		task_manager.spawn_handle(),
		overrides.clone(),
		50,
		50,
	));

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		let network = network.clone();
		let filter_pool = filter_pool.clone();
		let frontier_backend = frontier_backend.clone();
		let overrides = overrides.clone();
		let fee_history_cache = fee_history_cache.clone();
		let max_past_logs = cli.run.max_past_logs;

		Box::new(move |deny_unsafe, _| {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				graph: pool.pool().clone(),
				deny_unsafe,
				is_authority,
				enable_dev_signer,
				network: network.clone(),
				filter_pool: filter_pool.clone(),
				backend: frontier_backend.clone(),
				max_past_logs,
				fee_history_limit,
				fee_history_cache: fee_history_cache.clone(),
				command_sink: Some(command_sink.clone()),
				overrides: overrides.clone(),
				block_data_cache: block_data_cache.clone(),
			};

			Ok(crate::rpc::create_full(
				deps,
				subscription_task_executor.clone(),
			))
		})
	};

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: network.clone(),
		client: client.clone(),
		keystore: keystore_container.sync_keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		rpc_extensions_builder,
		backend: backend.clone(),
		system_rpc_tx,
		config,
		telemetry: telemetry.as_mut(),
	})?;

	task_manager.spawn_essential_handle().spawn(
		"frontier-mapping-sync-worker",
		None,
		MappingSyncWorker::new(
			client.import_notification_stream(),
			Duration::new(6, 0),
			client.clone(),
			backend.clone(),
			frontier_backend.clone(),
			SyncStrategy::Normal,
		)
		.for_each(|()| futures::future::ready(())),
	);

	// Spawn Frontier EthFilterApi maintenance task.
	if let Some(filter_pool) = filter_pool {
		// Each filter is allowed to stay in the pool for 100 blocks.
		const FILTER_RETAIN_THRESHOLD: u64 = 100;
		task_manager.spawn_essential_handle().spawn(
			"frontier-filter-pool",
			None,
			EthTask::filter_pool_task(Arc::clone(&client), filter_pool, FILTER_RETAIN_THRESHOLD),
		);
	}

	// Spawn Frontier FeeHistory cache maintenance task.
	task_manager.spawn_essential_handle().spawn(
		"frontier-fee-history",
		None,
		EthTask::fee_history_task(
			Arc::clone(&client),
			Arc::clone(&overrides),
			fee_history_cache,
			fee_history_limit,
		),
	);

	task_manager.spawn_essential_handle().spawn(
		"frontier-schema-cache-task",
		None,
		EthTask::ethereum_schema_cache_task(Arc::clone(&client), Arc::clone(&frontier_backend)),
	);

	#[cfg(feature = "manual-seal")]
	{
		let (block_import, sealing) = consensus_result;

		if role.is_authority() {
			let env = sc_basic_authorship::ProposerFactory::new(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool.clone(),
				prometheus_registry.as_ref(),
				telemetry.as_ref().map(|x| x.handle()),
			);

			let target_gas_price = cli.run.target_gas_price;

			// Background authorship future
			match sealing {
				Sealing::Manual => {
					let authorship_future =
						manual_seal::run_manual_seal(manual_seal::ManualSealParams {
							block_import,
							env,
							client,
							pool: transaction_pool.clone(),
							commands_stream,
							select_chain,
							consensus_data_provider: None,
							create_inherent_data_providers: move |_, ()| async move {
								let mock_timestamp = MockTimestampInherentDataProvider;

								let dynamic_fee = pallet_dynamic_fee::InherentDataProvider(
									U256::from(target_gas_price),
								);

								Ok((mock_timestamp, dynamic_fee))
							},
						});
					// we spawn the future on a background thread managed by service.
					task_manager.spawn_essential_handle().spawn_blocking(
						"manual-seal",
						None,
						authorship_future,
					);
				}
				Sealing::Instant => {
					let authorship_future =
						manual_seal::run_instant_seal(manual_seal::InstantSealParams {
							block_import,
							env,
							client: client.clone(),
							pool: transaction_pool.clone(),
							select_chain,
							consensus_data_provider: None,
							create_inherent_data_providers: move |_, ()| async move {
								let mock_timestamp = MockTimestampInherentDataProvider;

								let dynamic_fee = pallet_dynamic_fee::InherentDataProvider(
									U256::from(target_gas_price),
								);

								Ok((mock_timestamp, dynamic_fee))
							},
						});
					// we spawn the future on a background thread managed by service.
					task_manager.spawn_essential_handle().spawn_blocking(
						"instant-seal",
						None,
						authorship_future,
					);
				}
			};
		}
		log::info!("Manual Seal Ready");
	}

	#[cfg(feature = "aura")]
	{
		let (block_import, grandpa_link) = consensus_result;

		if role.is_authority() {
			let proposer_factory = sc_basic_authorship::ProposerFactory::new(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool,
				prometheus_registry.as_ref(),
				telemetry.as_ref().map(|x| x.handle()),
			);

			let can_author_with =
				sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

			let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
			let raw_slot_duration = slot_duration.slot_duration();
			let target_gas_price = cli.run.target_gas_price;

			let aura = sc_consensus_aura::start_aura::<AuraPair, _, _, _, _, _, _, _, _, _, _, _>(
				StartAuraParams {
					slot_duration,
					client: client.clone(),
					select_chain,
					block_import,
					proposer_factory,
					create_inherent_data_providers: move |_, ()| async move {
						let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

						let slot =
							sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_duration(
								*timestamp,
								raw_slot_duration,
							);

						let dynamic_fee =
							pallet_dynamic_fee::InherentDataProvider(U256::from(target_gas_price));

						Ok((timestamp, slot, dynamic_fee))
					},
					force_authoring,
					backoff_authoring_blocks,
					keystore: keystore_container.sync_keystore(),
					can_author_with,
					sync_oracle: network.clone(),
					justification_sync_link: network.clone(),
					block_proposal_slot_portion: SlotProportion::new(2f32 / 3f32),
					max_block_proposal_slot_portion: None,
					telemetry: telemetry.as_ref().map(|x| x.handle()),
				},
			)?;

			// the AURA authoring task is considered essential, i.e. if it
			// fails we take down the service with it.
			task_manager.spawn_essential_handle().spawn_blocking(
				"aura",
				Some("block-authoring"),
				aura,
			);
		}

		// if the node isn't actively participating in consensus then it doesn't
		// need a keystore, regardless of which protocol we use below.
		let keystore = if role.is_authority() {
			Some(keystore_container.sync_keystore())
		} else {
			None
		};

		let grandpa_config = sc_finality_grandpa::Config {
			// FIXME #1578 make this available through chainspec
			gossip_duration: Duration::from_millis(333),
			justification_period: 512,
			name: Some(name),
			observer_enabled: false,
			keystore,
			local_role: role,
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			protocol_name: grandpa_protocol_name,
		};

		if enable_grandpa {
			// start the full GRANDPA voter
			// NOTE: non-authorities could run the GRANDPA observer protocol, but at
			// this point the full voter should provide better guarantees of block
			// and vote data availability than the observer. The observer has not
			// been tested extensively yet and having most nodes in a network run it
			// could lead to finality stalls.
			let grandpa_config = sc_finality_grandpa::GrandpaParams {
				config: grandpa_config,
				link: grandpa_link,
				network,
				voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
				prometheus_registry,
				shared_voter_state: SharedVoterState::empty(),
				telemetry: telemetry.as_ref().map(|x| x.handle()),
			};

			// the GRANDPA voter task is considered infallible, i.e.
			// if it fails we take down the service with it.
			task_manager.spawn_essential_handle().spawn_blocking(
				"grandpa-voter",
				None,
				sc_finality_grandpa::run_grandpa_voter(grandpa_config)?,
			);
		}
	}

	network_starter.start_network();
	Ok(task_manager)
}
