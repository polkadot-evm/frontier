// This file is part of Frontier.

// Copyright (C) 2019-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use std::sync::Arc;
use sc_consensus_manual_seal::{self as manual_seal};
use frontier_test_runtime::{self, opaque::Block, RuntimeApi, Hash};
use sc_service::{error::Error as ServiceError, Configuration, ServiceComponents, TaskManager};
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;

// Our native executor instance.
native_executor_instance!(
	pub Executor,
	frontier_test_runtime::api::dispatch,
	frontier_test_runtime::native_version,
);

type FullClient = sc_service::TFullClient<Block, RuntimeApi, Executor>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

pub fn new_full_params(config: Configuration) -> Result<(
	sc_service::ServiceParams<
		Block, FullClient, sp_consensus::import_queue::BasicQueue<Block, sp_api::TransactionFor<FullClient, Block>>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		crate::rpc::IoHandler, FullBackend,
	>,
	FullSelectChain,
	futures::channel::mpsc::Receiver<sc_consensus_manual_seal::rpc::EngineCommand<Hash>>,
	sp_inherents::InherentDataProviders,
), ServiceError> {
	let inherent_data_providers = sp_inherents::InherentDataProviders::new();

	let (client, backend, keystore, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, Executor>(&config)?;
	let client = Arc::new(client);

	// Channel for the rpc handler to communicate with the authorship task.
	let (command_sink, commands_stream) = futures::channel::mpsc::channel(1000);

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let pool_api = sc_transaction_pool::FullChainApi::new(
		client.clone(), config.prometheus_registry(),
	);
	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		std::sync::Arc::new(pool_api),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
	);

	let import_queue = sc_consensus_manual_seal::import_queue(
		Box::new(client.clone()),
		&task_manager.spawn_handle(),
		config.prometheus_registry(),
	);

	let is_authority = config.role.is_authority();

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		let select_chain = select_chain.clone();
		let command_sink = command_sink.clone();

		Box::new(move |deny_unsafe| {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				select_chain: select_chain.clone(),
				deny_unsafe,
				is_authority,
				command_sink: command_sink.clone(),
			};

			crate::rpc::create_full(deps)
		})
	};

	let params = sc_service::ServiceParams {
		backend, client, import_queue, keystore, task_manager, transaction_pool, rpc_extensions_builder,
		config,
		block_announce_validator_builder: None,
		on_demand: None,
		remote_blockchain: None,
		finality_proof_provider: None,
		finality_proof_request_builder: Some(Box::new(sc_network::config::DummyFinalityProofRequestBuilder)),
	};

	Ok((params, select_chain, commands_stream, inherent_data_providers))
}

/// Builds a new service for a full client.
pub fn new_full(config: Configuration) -> Result<TaskManager, ServiceError> {
	let (
		params, select_chain, commands_stream, inherent_data_providers,
	) = new_full_params(config)?;

	inherent_data_providers
	.register_provider(sp_timestamp::InherentDataProvider)
	.map_err(Into::into)
	.map_err(sp_consensus::error::Error::InherentData)?;

	let (
		role, _force_authoring, _name, prometheus_registry,
		client, transaction_pool, _keystore,
	) = {
		let sc_service::ServiceParams {
			config, client, transaction_pool, keystore, ..
		} = &params;

		(
			config.role.clone(),
			config.force_authoring,
			config.network.node_name.clone(),
			config.prometheus_registry().cloned(),
			client.clone(), transaction_pool.clone(), keystore.clone(),
		)
	};

	let ServiceComponents {
		task_manager, ..
	} = sc_service::build(params)?;

	if role.is_authority() {
		let proposer = sc_basic_authorship::ProposerFactory::new(
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
		);

		// Background authorship future
		let authorship_future = manual_seal::run_manual_seal(
			Box::new(client.clone()),
			proposer,
			client.clone(),
			transaction_pool.pool().clone(),
			commands_stream,
			select_chain,
			inherent_data_providers,
		);

		// we spawn the future on a background thread managed by service.
		task_manager.spawn_essential_handle().spawn_blocking("manual-seal", authorship_future);
	}
	log::info!("Test Node Ready");

	Ok(task_manager)
}
