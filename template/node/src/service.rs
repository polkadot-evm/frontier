//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use std::{sync::{Arc, Mutex}, cell::RefCell, time::Duration, collections::{HashMap, BTreeMap}};
use fc_rpc_core::types::{FilterPool, PendingTransactions};
use sc_client_api::{ExecutorProvider, RemoteBackend, BlockchainEvents};
use sc_consensus_manual_seal::{self as manual_seal};
use fc_consensus::FrontierBlockImport;
use fc_mapping_sync::MappingSyncWorker;
use frontier_template_runtime::{self, opaque::Block, RuntimeApi, SLOT_DURATION};
use sc_service::{error::Error as ServiceError, Configuration, TaskManager, BasePath};
use sp_inherents::{InherentDataProviders, ProvideInherentData, InherentIdentifier, InherentData};
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;
use sp_consensus_aura::sr25519::{AuthorityPair as AuraPair};
use sc_finality_grandpa::SharedVoterState;
use sp_timestamp::InherentError;
use sc_telemetry::TelemetrySpan;
use sc_cli::SubstrateCli;
use futures::StreamExt;
use crate::cli::Sealing;

// Our native executor instance.
native_executor_instance!(
	pub Executor,
	frontier_template_runtime::api::dispatch,
	frontier_template_runtime::native_version,
);

type FullClient = sc_service::TFullClient<Block, RuntimeApi, Executor>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

pub enum ConsensusResult {
	Aura(
		sc_consensus_aura::AuraBlockImport<
			Block,
			FullClient,
			FrontierBlockImport<
				Block,
				sc_finality_grandpa::GrandpaBlockImport<FullBackend, Block, FullClient, FullSelectChain>,
				FullClient
			>,
			AuraPair
		>,
		sc_finality_grandpa::LinkHalf<Block, FullClient, FullSelectChain>
	),
	ManualSeal(FrontierBlockImport<Block, Arc<FullClient>, FullClient>, Sealing)
}

/// Provide a mock duration starting at 0 in millisecond for timestamp inherent.
/// Each call will increment timestamp by slot_duration making Aura think time has passed.
pub struct MockTimestampInherentDataProvider;

pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"timstap0";

thread_local!(static TIMESTAMP: RefCell<u64> = RefCell::new(0));

impl ProvideInherentData for MockTimestampInherentDataProvider {
	fn inherent_identifier(&self) -> &'static InherentIdentifier {
		&INHERENT_IDENTIFIER
	}

	fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		TIMESTAMP.with(|x| {
			*x.borrow_mut() += SLOT_DURATION;
			inherent_data.put_data(INHERENT_IDENTIFIER, &*x.borrow())
		})
	}

	fn error_to_string(&self, error: &[u8]) -> Option<String> {
		InherentError::try_from(&INHERENT_IDENTIFIER, error).map(|e| format!("{:?}", e))
	}
}

pub fn open_frontier_backend(config: &Configuration) -> Result<Arc<fc_db::Backend<Block>>, String> {
	let config_dir = config.base_path.as_ref()
		.map(|base_path| base_path.config_dir(config.chain_spec.id()))
		.unwrap_or_else(|| {
			BasePath::from_project("", "", &crate::cli::Cli::executable_name())
				.config_dir(config.chain_spec.id())
		});
	let database_dir = config_dir.join("frontier").join("db");

	Ok(Arc::new(fc_db::Backend::<Block>::new(&fc_db::DatabaseSettings {
		source: fc_db::DatabaseSettingsSrc::RocksDb {
			path: database_dir,
			cache_size: 0,
		}
	})?))
}

pub fn new_partial(config: &Configuration, sealing: Option<Sealing>) -> Result<
	sc_service::PartialComponents<
		FullClient, FullBackend, FullSelectChain,
		sp_consensus::import_queue::BasicQueue<Block, sp_api::TransactionFor<FullClient, Block>>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(ConsensusResult, PendingTransactions, Option<TelemetrySpan>, Option<FilterPool>, Arc<fc_db::Backend<Block>>),
>, ServiceError> {
	let inherent_data_providers = sp_inherents::InherentDataProviders::new();

	let (client, backend, keystore_container, task_manager, telemetry_span) =
		sc_service::new_full_parts::<Block, RuntimeApi, Executor>(&config)?;
	let client = Arc::new(client);

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
	);

	let pending_transactions: PendingTransactions
		= Some(Arc::new(Mutex::new(HashMap::new())));

	let filter_pool: Option<FilterPool>
		= Some(Arc::new(Mutex::new(BTreeMap::new())));

	let frontier_backend = open_frontier_backend(config)?;

	if let Some(sealing) = sealing {
		inherent_data_providers
			.register_provider(MockTimestampInherentDataProvider)
			.map_err(Into::into)
			.map_err(sp_consensus::error::Error::InherentData)?;

		let frontier_block_import = FrontierBlockImport::new(
			client.clone(),
			client.clone(),
			frontier_backend.clone(),
		);

		let import_queue = sc_consensus_manual_seal::import_queue(
			Box::new(frontier_block_import.clone()),
			&task_manager.spawn_handle(),
			config.prometheus_registry(),
		);

		return Ok(sc_service::PartialComponents {
			client, backend, task_manager, import_queue, keystore_container,
			select_chain, transaction_pool, inherent_data_providers,
			other: (ConsensusResult::ManualSeal(frontier_block_import, sealing), pending_transactions, telemetry_span, filter_pool, frontier_backend)
		})
	}

	let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
		client.clone(), &(client.clone() as Arc<_>), select_chain.clone(),
	)?;

	let frontier_block_import = FrontierBlockImport::new(
		grandpa_block_import.clone(),
		client.clone(),
		frontier_backend.clone(),
	);

	let aura_block_import = sc_consensus_aura::AuraBlockImport::<_, _, _, AuraPair>::new(
		frontier_block_import, client.clone(),
	);

	let import_queue = sc_consensus_aura::import_queue::<_, _, _, AuraPair, _, _>(
		sc_consensus_aura::slot_duration(&*client)?,
		aura_block_import.clone(),
		Some(Box::new(grandpa_block_import.clone())),
		client.clone(),
		inherent_data_providers.clone(),
		&task_manager.spawn_handle(),
		config.prometheus_registry(),
		sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
	)?;

	Ok(sc_service::PartialComponents {
		client, backend, task_manager, import_queue, keystore_container,
		select_chain, transaction_pool, inherent_data_providers,
		other: (ConsensusResult::Aura(aura_block_import, grandpa_link), pending_transactions, telemetry_span, filter_pool, frontier_backend)
	})
}

/// Builds a new service for a full client.
pub fn new_full(
	config: Configuration,
	sealing: Option<Sealing>,
	enable_dev_signer: bool,
) -> Result<TaskManager, ServiceError> {
	let sc_service::PartialComponents {
		client, backend, mut task_manager, import_queue, keystore_container,
		select_chain, transaction_pool, inherent_data_providers,
		other: (consensus_result, pending_transactions, telemetry_span, filter_pool, frontier_backend),
	} = new_partial(&config, sealing)?;

	let (network, network_status_sinks, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			on_demand: None,
			block_announce_validator_builder: None,
		})?;

	// Channel for the rpc handler to communicate with the authorship task.
	let (command_sink, commands_stream) = futures::channel::mpsc::channel(1000);

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config, backend.clone(), task_manager.spawn_handle(), client.clone(), network.clone(),
		);
	}

	let role = config.role.clone();
	let force_authoring = config.force_authoring;
	let backoff_authoring_blocks: Option<()> = None;
	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();
	let is_authority = role.is_authority();
	let subscription_task_executor = sc_rpc::SubscriptionTaskExecutor::new(task_manager.spawn_handle());

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		let network = network.clone();
		let pending = pending_transactions.clone();
		let filter_pool = filter_pool.clone();
		let frontier_backend = frontier_backend.clone();

		Box::new(move |deny_unsafe, _| {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				deny_unsafe,
				is_authority,
				enable_dev_signer,
				network: network.clone(),
				pending_transactions: pending.clone(),
				filter_pool: filter_pool.clone(),
				backend: frontier_backend.clone(),
				command_sink: Some(command_sink.clone())
			};
			crate::rpc::create_full(
				deps,
				subscription_task_executor.clone()
			)
		})
	};

	task_manager.spawn_essential_handle().spawn(
		"frontier-mapping-sync-worker",
		MappingSyncWorker::new(
			client.import_notification_stream(),
			Duration::new(6, 0),
			client.clone(),
			backend.clone(),
			frontier_backend.clone(),
		).for_each(|()| futures::future::ready(()))
	);

	let (_rpc_handlers, telemetry_connection_notifier) = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: network.clone(),
		client: client.clone(),
		keystore: keystore_container.sync_keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		rpc_extensions_builder: rpc_extensions_builder,
		on_demand: None,
		remote_blockchain: None,
		backend, network_status_sinks, system_rpc_tx, config, telemetry_span,
	})?;

	// Spawn Frontier EthFilterApi maintenance task.
	if filter_pool.is_some() {
		// Each filter is allowed to stay in the pool for 100 blocks.
		const FILTER_RETAIN_THRESHOLD: u64 = 100;
		task_manager.spawn_essential_handle().spawn(
			"frontier-filter-pool",
			client.import_notification_stream().for_each(move |notification| {
				if let Ok(locked) = &mut filter_pool.clone().unwrap().lock() {
					let imported_number: u64 = notification.header.number as u64;
					for (k, v) in locked.clone().iter() {
						let lifespan_limit = v.at_block + FILTER_RETAIN_THRESHOLD;
						if lifespan_limit <= imported_number {
							locked.remove(&k);
						}
					}
				}
				futures::future::ready(())
			})
		);
	}

	// Spawn Frontier pending transactions maintenance task (as essential, otherwise we leak).
	if pending_transactions.is_some() {
		use fp_consensus::{FRONTIER_ENGINE_ID, ConsensusLog};
		use sp_runtime::generic::OpaqueDigestItemId;

		const TRANSACTION_RETAIN_THRESHOLD: u64 = 5;
		task_manager.spawn_essential_handle().spawn(
			"frontier-pending-transactions",
			client.import_notification_stream().for_each(move |notification| {
				if let Ok(locked) = &mut pending_transactions.clone().unwrap().lock() {
					// As pending transactions have a finite lifespan anyway
					// we can ignore MultiplePostRuntimeLogs error checks.
					let mut frontier_log: Option<_> = None;
					for log in notification.header.digest.logs {
						let log = log.try_to::<ConsensusLog>(OpaqueDigestItemId::Consensus(&FRONTIER_ENGINE_ID));
						if let Some(log) = log {
							frontier_log = Some(log);
						}
					}

					let imported_number: u64 = notification.header.number as u64;

					let post_hashes = frontier_log.map(|l| {
						match l {
							ConsensusLog::PostHashes(post_hashes) => post_hashes,
							ConsensusLog::PreBlock(block) => fp_consensus::PostHashes::from_block(block),
							ConsensusLog::PostBlock(block) => fp_consensus::PostHashes::from_block(block),
						}
					});

					if let Some(post_hashes) = post_hashes {
						// Retain all pending transactions that were not
						// processed in the current block.
						locked.retain(|&k, _| !post_hashes.transaction_hashes.contains(&k));
					}
					locked.retain(|_, v| {
						// Drop all the transactions that exceeded the given lifespan.
						let lifespan_limit = v.at_block + TRANSACTION_RETAIN_THRESHOLD;
						lifespan_limit > imported_number
					});
				}
				futures::future::ready(())
			})
		);
	}

	match consensus_result {
		ConsensusResult::ManualSeal(block_import, sealing) => {
			if role.is_authority() {
				let env = sc_basic_authorship::ProposerFactory::new(
					task_manager.spawn_handle(),
					client.clone(),
					transaction_pool.clone(),
					prometheus_registry.as_ref(),
				);

				// Background authorship future
				match sealing {
					Sealing::Manual => {
						let authorship_future = manual_seal::run_manual_seal(
							manual_seal::ManualSealParams {
								block_import,
								env,
								client,
								pool: transaction_pool.pool().clone(),
								commands_stream,
								select_chain,
								consensus_data_provider: None,
								inherent_data_providers,
							}
						);
						// we spawn the future on a background thread managed by service.
						task_manager.spawn_essential_handle().spawn_blocking("manual-seal", authorship_future);
					},
					Sealing::Instant => {
						let authorship_future = manual_seal::run_instant_seal(
							manual_seal::InstantSealParams {
								block_import,
								env,
								client: client.clone(),
								pool: transaction_pool.pool().clone(),
								select_chain,
								consensus_data_provider: None,
								inherent_data_providers,
							}
						);
						// we spawn the future on a background thread managed by service.
						task_manager.spawn_essential_handle().spawn_blocking("instant-seal", authorship_future);
					}
				};

			}
			log::info!("Manual Seal Ready");
		},
		ConsensusResult::Aura(aura_block_import, grandpa_link) => {
			if role.is_authority() {
				let proposer = sc_basic_authorship::ProposerFactory::new(
					task_manager.spawn_handle(),
					client.clone(),
					transaction_pool.clone(),
					prometheus_registry.as_ref(),
				);

				let can_author_with =
					sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());
				let aura = sc_consensus_aura::start_aura::<_, _, _, _, _, AuraPair, _, _, _, _>(
					sc_consensus_aura::slot_duration(&*client)?,
					client.clone(),
					select_chain,
					aura_block_import,
					proposer,
					network.clone(),
					inherent_data_providers.clone(),
					force_authoring,
					backoff_authoring_blocks,
					keystore_container.sync_keystore(),
					can_author_with,
				)?;

				// the AURA authoring task is considered essential, i.e. if it
				// fails we take down the service with it.
				task_manager.spawn_essential_handle().spawn_blocking("aura", aura);

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
					is_authority: role.is_network_authority(),
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
						telemetry_on_connect: telemetry_connection_notifier.map(|x| x.on_connect_stream()),
						voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
						prometheus_registry,
						shared_voter_state: SharedVoterState::empty(),
					};

					// the GRANDPA voter task is considered infallible, i.e.
					// if it fails we take down the service with it.
					task_manager.spawn_essential_handle().spawn_blocking(
						"grandpa-voter",
						sc_finality_grandpa::run_grandpa_voter(grandpa_config)?
					);
				}
			}
		}
	}

	network_starter.start_network();
	Ok(task_manager)
}

// FIXME: #238 Light client does not have a complete import pipeline or support manual/instant seal.
/// Builds a new service for a light client.
pub fn new_light(config: Configuration) -> Result<TaskManager, ServiceError> {
	let (client, backend, keystore_container, mut task_manager, on_demand, telemetry_span) =
		sc_service::new_light_parts::<Block, RuntimeApi, Executor>(&config)?;

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = Arc::new(sc_transaction_pool::BasicPool::new_light(
		config.transaction_pool.clone(),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
		on_demand.clone(),
	));

	let (grandpa_block_import, _) = sc_finality_grandpa::block_import(
		client.clone(),
		&(client.clone() as Arc<_>),
		select_chain.clone(),
	)?;

	let import_queue = sc_consensus_aura::import_queue::<_, _, _, AuraPair, _, _>(
		sc_consensus_aura::slot_duration(&*client)?,
		grandpa_block_import.clone(),
		Some(Box::new(grandpa_block_import)),
		client.clone(),
		InherentDataProviders::new(),
		&task_manager.spawn_handle(),
		config.prometheus_registry(),
		sp_consensus::NeverCanAuthor,
	)?;

	let light_deps = crate::rpc::LightDeps {
		remote_blockchain: backend.remote_blockchain(),
		fetcher: on_demand.clone(),
		client: client.clone(),
		pool: transaction_pool.clone(),
	};

	let rpc_extensions = crate::rpc::create_light(light_deps);

	let (network, network_status_sinks, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			on_demand: Some(on_demand.clone()),
			block_announce_validator_builder: None,
		})?;

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config, backend.clone(), task_manager.spawn_handle(), client.clone(), network.clone(),
		);
	}

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		remote_blockchain: Some(backend.remote_blockchain()),
		transaction_pool,
		task_manager: &mut task_manager,
		on_demand: Some(on_demand),
		rpc_extensions_builder: Box::new(sc_service::NoopRpcExtensionBuilder(rpc_extensions)),
		config,
		client,
		keystore: keystore_container.sync_keystore(),
		backend,
		network,
		network_status_sinks,
		system_rpc_tx,
		telemetry_span,
	})?;

	network_starter.start_network();

	Ok(task_manager)
}
