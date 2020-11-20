//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use std::{sync::Arc, cell::RefCell, time::Duration};
use sc_client_api::{ExecutorProvider, RemoteBackend};
use sc_consensus_manual_seal::{self as manual_seal};
use fc_consensus::FrontierBlockImport;
use frontier_template_runtime::{self, opaque::Block, RuntimeApi, SLOT_DURATION};
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use sp_inherents::{InherentDataProviders, ProvideInherentData, InherentIdentifier, InherentData};
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;
use sp_consensus_aura::sr25519::{AuthorityPair as AuraPair};
use sc_finality_grandpa::{
	FinalityProofProvider as GrandpaFinalityProofProvider, SharedVoterState,
};
use sp_timestamp::InherentError;
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

pub fn new_partial(config: &Configuration, sealing: Option<Sealing>) -> Result<
	sc_service::PartialComponents<
		FullClient, FullBackend, FullSelectChain,
		sp_consensus::import_queue::BasicQueue<Block, sp_api::TransactionFor<FullClient, Block>>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		ConsensusResult,
>, ServiceError> {
	let inherent_data_providers = sp_inherents::InherentDataProviders::new();

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, Executor>(&config)?;
	let client = Arc::new(client);

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
	);

	if let Some(sealing) = sealing {
		inherent_data_providers
			.register_provider(MockTimestampInherentDataProvider)
			.map_err(Into::into)
			.map_err(sp_consensus::error::Error::InherentData)?;

		let frontier_block_import = FrontierBlockImport::new(
			client.clone(),
			client.clone(),
			true,
		);

		let import_queue = sc_consensus_manual_seal::import_queue(
			Box::new(frontier_block_import.clone()),
			&task_manager.spawn_handle(),
			config.prometheus_registry(),
		);

		return Ok(sc_service::PartialComponents {
			client, backend, task_manager, import_queue, keystore_container,
			select_chain, transaction_pool, inherent_data_providers,
			other: ConsensusResult::ManualSeal(frontier_block_import, sealing)
		})
	}

	let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
		client.clone(), &(client.clone() as Arc<_>), select_chain.clone(),
	)?;

	let frontier_block_import = FrontierBlockImport::new(
		grandpa_block_import.clone(),
		client.clone(),
		true
	);

	let aura_block_import = sc_consensus_aura::AuraBlockImport::<_, _, _, AuraPair>::new(
		frontier_block_import, client.clone(),
	);

	let import_queue = sc_consensus_aura::import_queue::<_, _, _, AuraPair, _, _>(
		sc_consensus_aura::slot_duration(&*client)?,
		aura_block_import.clone(),
		Some(Box::new(grandpa_block_import.clone())),
		None,
		client.clone(),
		inherent_data_providers.clone(),
		&task_manager.spawn_handle(),
		config.prometheus_registry(),
		sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
	)?;

	Ok(sc_service::PartialComponents {
		client, backend, task_manager, import_queue, keystore_container,
		select_chain, transaction_pool, inherent_data_providers,
		other: ConsensusResult::Aura(aura_block_import, grandpa_link)
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
		select_chain, transaction_pool, inherent_data_providers, other: consensus_result,
	} = new_partial(&config, sealing)?;

	let (network, network_status_sinks, system_rpc_tx, network_starter) = match consensus_result {
		ConsensusResult::ManualSeal(_, _) => {
			sc_service::build_network(sc_service::BuildNetworkParams {
				config: &config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				spawn_handle: task_manager.spawn_handle(),
				import_queue,
				on_demand: None,
				block_announce_validator_builder: None,
				finality_proof_request_builder: Some(Box::new(sc_network::config::DummyFinalityProofRequestBuilder)),
				finality_proof_provider: None,
			})?
		},
		ConsensusResult::Aura(_, _) => {
			sc_service::build_network(sc_service::BuildNetworkParams {
				config: &config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				spawn_handle: task_manager.spawn_handle(),
				import_queue,
				on_demand: None,
				block_announce_validator_builder: None,
				finality_proof_request_builder: None,
				finality_proof_provider: Some(GrandpaFinalityProofProvider::new_for_service(backend.clone(), client.clone())),
			})?
		}
	};

	// Channel for the rpc handler to communicate with the authorship task.
	let (command_sink, commands_stream) = futures::channel::mpsc::channel(1000);

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config, backend.clone(), task_manager.spawn_handle(), client.clone(), network.clone(),
		);
	}

	let role = config.role.clone();
	let force_authoring = config.force_authoring;
	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();
	let telemetry_connection_sinks = sc_service::TelemetryConnectionSinks::default();
	let is_authority = role.is_authority();
	let subscription_task_executor = sc_rpc::SubscriptionTaskExecutor::new(task_manager.spawn_handle());

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		let network = network.clone();
		Box::new(move |deny_unsafe, _| {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				deny_unsafe,
				is_authority,
				enable_dev_signer,
				network: network.clone(),
				command_sink: Some(command_sink.clone())
			};
			crate::rpc::create_full(
				deps,
				subscription_task_executor.clone()
			)
		})
	};

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: network.clone(),
		client: client.clone(),
		keystore: keystore_container.sync_keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		telemetry_connection_sinks: telemetry_connection_sinks.clone(),
		rpc_extensions_builder: rpc_extensions_builder,
		on_demand: None,
		remote_blockchain: None,
		backend, network_status_sinks, system_rpc_tx, config,
	})?;

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
				let aura = sc_consensus_aura::start_aura::<_, _, _, _, _, AuraPair, _, _, _>(
					sc_consensus_aura::slot_duration(&*client)?,
					client.clone(),
					select_chain,
					aura_block_import,
					proposer,
					network.clone(),
					inherent_data_providers.clone(),
					force_authoring,
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
						telemetry_on_connect: Some(telemetry_connection_sinks.on_connect_stream()),
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
				} else {
					sc_finality_grandpa::setup_disabled_grandpa(network)?;
				}
			}
		}
	}

	network_starter.start_network();
	Ok(task_manager)
}

/// Builds a new service for a light client.
pub fn new_light(config: Configuration) -> Result<TaskManager, ServiceError> {
	let (client, backend, keystore_container, mut task_manager, on_demand) =
		sc_service::new_light_parts::<Block, RuntimeApi, Executor>(&config)?;

	let transaction_pool = Arc::new(sc_transaction_pool::BasicPool::new_light(
		config.transaction_pool.clone(),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
		on_demand.clone(),
	));

	let grandpa_block_import = sc_finality_grandpa::light_block_import(
		client.clone(), backend.clone(), &(client.clone() as Arc<_>),
		Arc::new(on_demand.checker().clone()) as Arc<_>,
	)?;

	let finality_proof_import = grandpa_block_import.clone();
	let finality_proof_request_builder =
		finality_proof_import.create_finality_proof_request_builder();

	let import_queue = sc_consensus_aura::import_queue::<_, _, _, AuraPair, _, _>(
		sc_consensus_aura::slot_duration(&*client)?,
		grandpa_block_import,
		None,
		Some(Box::new(finality_proof_import)),
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

	let finality_proof_provider =
		Arc::new(GrandpaFinalityProofProvider::new(backend.clone(), client.clone() as Arc<_>));

	let (network, network_status_sinks, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			on_demand: Some(on_demand.clone()),
			block_announce_validator_builder: None,
			finality_proof_request_builder: Some(finality_proof_request_builder),
			finality_proof_provider: Some(finality_proof_provider),
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
		telemetry_connection_sinks: sc_service::TelemetryConnectionSinks::default(),
		config,
		client,
		keystore: keystore_container.sync_keystore(),
		backend,
		network,
		network_status_sinks,
		system_rpc_tx,
	})?;

	network_starter.start_network();

	Ok(task_manager)
}
