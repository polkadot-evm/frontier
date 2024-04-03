// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
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


#![warn(unused_crate_dependencies)]

use std::{marker::PhantomData, sync::Arc};

// Substrate
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus::Error as ConsensusError;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
// Frontier
use fp_consensus::{ensure_log, FindLogError};
use fp_rpc::EthereumRuntimeRPCApi;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Multiple runtime Ethereum blocks, rejecting!")]
	MultipleRuntimeLogs,
	#[error("Runtime Ethereum block not found, rejecting!")]
	NoRuntimeLog,
	#[error("Cannot access the runtime at genesis, rejecting!")]
	RuntimeApiCallFailed,
}

impl From<Error> for String {
	fn from(error: Error) -> String {
		error.to_string()
	}
}

impl From<FindLogError> for Error {
	fn from(error: FindLogError) -> Error {
		match error {
			FindLogError::NotFound => Error::NoRuntimeLog,
			FindLogError::MultipleLogs => Error::MultipleRuntimeLogs,
		}
	}
}

impl From<Error> for ConsensusError {
	fn from(error: Error) -> ConsensusError {
		ConsensusError::ClientImport(error.to_string())
	}
}

pub struct FrontierBlockImport<B: BlockT, I, C> {
	inner: I,
	client: Arc<C>,
	_marker: PhantomData<B>,
}

impl<Block: BlockT, I: Clone + BlockImport<Block>, C> Clone for FrontierBlockImport<Block, I, C> {
	fn clone(&self) -> Self {
		FrontierBlockImport {
			inner: self.inner.clone(),
			client: self.client.clone(),
			_marker: PhantomData,
		}
	}
}

impl<B, I, C> FrontierBlockImport<B, I, C>
where
	B: BlockT,
	I: BlockImport<B>,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
{
	pub fn new(inner: I, client: Arc<C>) -> Self {
		Self {
			inner,
			client,
			_marker: PhantomData,
		}
	}
}

#[async_trait::async_trait]
impl<B, I, C> BlockImport<B> for FrontierBlockImport<B, I, C>
where
	B: BlockT,
	I: BlockImport<B> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
{
	type Error = ConsensusError;

	async fn check_block(
		&mut self,
		block: BlockCheckParams<B>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await.map_err(Into::into)
	}

	async fn import_block(
		&mut self,
		block: BlockImportParams<B>,
	) -> Result<ImportResult, Self::Error> {
		// We validate that there are only one frontier log. No other
		// actions are needed and mapping syncing is delegated to a separate
		// worker.
		ensure_log(block.header.digest()).map_err(Error::from)?;

		self.inner.import_block(block).await.map_err(Into::into)
	}
}
