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

use frontier_test_runtime::{self};
use sc_consensus_manual_seal::{self as manual_seal};
use sc_service::{
	error::{Error as ServiceError}, Configuration, ServiceComponents,
	TaskManager,
};

use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;

// Our native executor instance.
native_executor_instance!(
	pub Executor,
	frontier_test_runtime::api::dispatch,
	frontier_test_runtime::native_version,
);

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
macro_rules! new_full_start {
	($config:expr) => {{

		let inherent_data_providers = sp_inherents::InherentDataProviders::new();


		// Channel for the rpc handler to communicate with the authorship task.
		let (command_sink, commands_stream) = futures::channel::mpsc::channel(1000);

		let builder = sc_service::ServiceBuilder::new_full::<
			frontier_test_runtime::opaque::Block, frontier_test_runtime::RuntimeApi, crate::service::Executor
		>($config)?
			.with_select_chain(|_config, backend| {
				Ok(sc_consensus::LongestChain::new(backend.clone()))
			})?
			.with_transaction_pool(|builder| {
				let pool_api = sc_transaction_pool::FullChainApi::new(
					builder.client().clone(),
					None,
				);
				Ok(sc_transaction_pool::BasicPool::new_full(
					builder.config().transaction_pool.clone(),
					std::sync::Arc::new(pool_api),
					builder.prometheus_registry(),
					builder.spawn_handle(),
					builder.client().clone(),
				))
			})?
			.with_import_queue(
				|_config, client, _select_chain, _transaction_pool, spawn_task_handle, registry| {
				Ok(sc_consensus_manual_seal::import_queue(
					Box::new(client),
					spawn_task_handle,
					registry,
				))
			})?
			.with_rpc_extensions_builder(|builder| {
				let client = builder.client().clone();
				let is_authority: bool = builder.config().role.is_authority();
				let pool = builder.pool().clone();
				let select_chain = builder.select_chain().cloned()
					.expect("SelectChain is present for full services or set up failed; qed.");

				Ok(move |deny_unsafe| {
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
			})?;

		(builder, commands_stream, inherent_data_providers)
	}}
}

/// Builds a new service for a full client.
pub fn new_full(config: Configuration) -> Result<TaskManager, ServiceError> {
	let is_authority = config.role.is_authority();



	let (builder, commands_stream, inherent_data_providers) = new_full_start!(config);

	inherent_data_providers
		.register_provider(sp_timestamp::InherentDataProvider)
		.map_err(Into::into)
		.map_err(sp_consensus::error::Error::InherentData)?;

	let ServiceComponents {
		client, transaction_pool, task_manager, select_chain,
		prometheus_registry, ..
	} = builder
		.build_full()?;

	if is_authority {
		// Proposer object for block authorship.
		let proposer = sc_basic_authorship::ProposerFactory::new(
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
		);

		// Background authorship future.
		let authorship_future = manual_seal::run_manual_seal(
			Box::new(client.clone()),
			proposer,
			client.clone(),
			transaction_pool.pool().clone(),
			commands_stream,
			select_chain.unwrap(),
			inherent_data_providers,
		);

		// we spawn the future on a background thread managed by service.
		task_manager.spawn_essential_handle().spawn_blocking("manual-seal", authorship_future);
	};
	log::info!("Test Node Ready");

	Ok(task_manager)
}
