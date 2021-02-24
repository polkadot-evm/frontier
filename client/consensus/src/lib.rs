// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::sync::Arc;
use std::collections::HashMap;
use std::marker::PhantomData;
use fp_consensus::{FRONTIER_ENGINE_ID, ConsensusLog};
use fp_rpc::EthereumRuntimeRPCApi;
use sc_client_api::{BlockOf, backend::AuxStore};
use sp_blockchain::{HeaderBackend, ProvideCache, well_known_cache_keys::Id as CacheKeyId};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_runtime::generic::OpaqueDigestItemId;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, One, Zero};
use sp_api::{ProvideRuntimeApi, BlockId};
use sp_consensus::{
	BlockImportParams, Error as ConsensusError, BlockImport,
	BlockCheckParams, ImportResult,
};
use log::*;
use sc_client_api;

#[derive(derive_more::Display, Debug)]
pub enum Error {
	#[display(fmt = "Multiple post-runtime Ethereum blocks, rejecting!")]
	MultiplePostRuntimeLogs,
	#[display(fmt = "Post-runtime Ethereum block not found, rejecting!")]
	NoPostRuntimeLog,
	#[display(fmt = "Cannot access the runtime at genesis, rejecting!")]
	RuntimeApiCallFailed,
}

impl From<Error> for String {
	fn from(error: Error) -> String {
		error.to_string()
	}
}

impl std::convert::From<Error> for ConsensusError {
	fn from(error: Error) -> ConsensusError {
		ConsensusError::ClientImport(error.to_string())
	}
}

pub struct FrontierBlockImport<B: BlockT, I, C> {
	inner: I,
	client: Arc<C>,
	backend: Arc<fc_db::Backend<B>>,
	enabled: bool,
	_marker: PhantomData<B>,
}

impl<Block: BlockT, I: Clone + BlockImport<Block>, C> Clone for FrontierBlockImport<Block, I, C> {
	fn clone(&self) -> Self {
		FrontierBlockImport {
			inner: self.inner.clone(),
			client: self.client.clone(),
			backend: self.backend.clone(),
			enabled: self.enabled,
			_marker: PhantomData,
		}
	}
}

impl<B, I, C> FrontierBlockImport<B, I, C> where
	B: BlockT,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<B>,
	C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>,
{
	pub fn new(
		inner: I,
		client: Arc<C>,
		backend: Arc<fc_db::Backend::<B>>,
		enabled: bool,
	) -> Self {
		Self {
			inner,
			client,
			backend,
			enabled,
			_marker: PhantomData,
		}
	}
}

impl<B, I, C> BlockImport<B> for FrontierBlockImport<B, I, C> where
	B: BlockT,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<B>,
	C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>,
{
	type Error = ConsensusError;
	type Transaction = sp_api::TransactionFor<C, B>;

	fn check_block(
		&mut self,
		block: BlockCheckParams<B>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).map_err(Into::into)
	}

	fn import_block(
		&mut self,
		mut block: BlockImportParams<B, Self::Transaction>,
		new_cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		macro_rules! insert_closure {
			() => (
				|insert| block.auxiliary.extend(
					insert.iter().map(|(k, v)| (k.to_vec(), Some(v.to_vec())))
				)
			)
		}

		let client = self.client.clone();

		if self.enabled {
			let log = find_frontier_log::<B>(&block.header)?;
			let hash = block.post_hash();
			let post_hashes = match log {
				ConsensusLog::PostHashes(post_hashes) => post_hashes,
				ConsensusLog::PreBlock(block) => fp_consensus::PostHashes::from_block(block),
				ConsensusLog::PostBlock(block) => fp_consensus::PostHashes::from_block(block),
			};

			let mapping_commitment = fc_db::MappingCommitment {
				block_hash: hash,
				ethereum_block_hash: post_hashes.block_hash,
				ethereum_transaction_hashes: post_hashes.transaction_hashes,
			};
			let res = self.backend.mapping_db().write_hashes(mapping_commitment);
			if res.is_err() { trace!(target: "frontier-consensus", "{:?}", res); }

			// On importing block 1 we also map the genesis block in the auxiliary.
			if block.header.number().clone() == One::one() {
				let id = BlockId::Number(Zero::zero());
				if let Ok(Some(header)) = client.header(id) {
					let block = self.client.runtime_api().current_block(&id)
						.map_err(|_| Error::RuntimeApiCallFailed)?;
					let block_hash = block.unwrap().header.hash(); // TODO: shouldn't use unwrap
					let mapping_commitment = fc_db::MappingCommitment::<B> {
						block_hash: header.hash(),
						ethereum_block_hash: block_hash,
						ethereum_transaction_hashes: Vec::new(),
					};
					let res = self.backend.mapping_db().write_hashes(mapping_commitment);
					if res.is_err() { trace!(target: "frontier-consensus", "{:?}", res); }
				}
			}
		}

		self.inner.import_block(block, new_cache).map_err(Into::into)
	}
}

fn find_frontier_log<B: BlockT>(
	header: &B::Header,
) -> Result<ConsensusLog, Error> {
	let mut frontier_log: Option<_> = None;
	for log in header.digest().logs() {
		trace!(target: "frontier-consensus", "Checking log {:?}, looking for ethereum block.", log);
		let log = log.try_to::<ConsensusLog>(OpaqueDigestItemId::Consensus(&FRONTIER_ENGINE_ID));
		match (log, frontier_log.is_some()) {
			(Some(_), true) =>
				return Err(Error::MultiplePostRuntimeLogs),
			(Some(log), false) => frontier_log = Some(log),
			_ => trace!(target: "frontier-consensus", "Ignoring digest not meant for us"),
		}
	}

	Ok(frontier_log.ok_or(Error::NoPostRuntimeLog)?)
}
