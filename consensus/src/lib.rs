// This file is part of Frontier.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

mod aux_schema;

pub use crate::aux_schema::{load_block_hash, load_transaction_metadata, load_logs};

use std::sync::Arc;
use std::collections::HashMap;
use std::marker::PhantomData;
use codec::Decode;
use frontier_consensus_primitives::{FRONTIER_ENGINE_ID, ConsensusLog};
use sc_client_api::{BlockOf, backend::AuxStore, StorageProvider, Backend, StateBackend};
use sp_blockchain::{HeaderBackend, ProvideCache, well_known_cache_keys::Id as CacheKeyId};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_runtime::generic::{OpaqueDigestItemId, BlockId};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, BlakeTwo256, UniqueSaturatedInto, Saturating, One};
use sp_api::ProvideRuntimeApi;
use sp_core::{H256, storage::StorageKey};
use sp_io::hashing::twox_128;
use sp_consensus::{
	BlockImportParams, Error as ConsensusError, BlockImport,
	BlockCheckParams, ImportResult,
};
use frontier_rpc_primitives::TransactionStatus;
use log::*;
use sc_client_api;

#[derive(derive_more::Display, Debug)]
pub enum Error {
	#[display(fmt = "Multiple post-runtime Ethereum blocks, rejecting!")]
	MultiplePostRuntimeLogs,
	#[display(fmt = "Post-runtime Ethereum block not found, rejecting!")]
	NoPostRuntimeLog,
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

pub struct FrontierBlockImport<B: BlockT, I, C, BE> {
	inner: I,
	client: Arc<C>,
	enabled: bool,
	_marker: PhantomData<(B, BE)>,
}

impl<Block: BlockT, I: Clone + BlockImport<Block>, C, BE> Clone for FrontierBlockImport<Block, I, C, BE> {
	fn clone(&self) -> Self {
		FrontierBlockImport {
			inner: self.inner.clone(),
			client: self.client.clone(),
			enabled: self.enabled,
			_marker: PhantomData,
		}
	}
}

impl<B, I, C, BE> FrontierBlockImport<B, I, C, BE> where
	B: BlockT,
	BE: Backend<B>,
	BE::State: StateBackend<BlakeTwo256>,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf + StorageProvider<B,BE>,
	C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>,
{
	pub fn new(
		inner: I,
		client: Arc<C>,
		enabled: bool,
	) -> Self {
		Self {
			inner,
			client,
			enabled,
			_marker: PhantomData,
		}
	}
}

impl<B, I, C, BE> BlockImport<B> for FrontierBlockImport<B, I, C, BE> where
	B: BlockT,
	BE: Backend<B>,
	BE::State: StateBackend<BlakeTwo256>,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf + StorageProvider<B,BE>,
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

			match log {
				ConsensusLog::EndBlock {
					block_hash, transaction_hashes,
				} => {
					aux_schema::write_block_hash(client.as_ref(), block_hash, hash, insert_closure!());

					for (index, transaction_hash) in transaction_hashes.into_iter().enumerate() {
						aux_schema::write_transaction_metadata(
							transaction_hash,
							(block_hash, index as u32),
							insert_closure!(),
						);
					}

					// Store already processed TransactionStatus by block number.
					if *block.header.number() > One::one() {
						let number = UniqueSaturatedInto::<u32>::unique_saturated_into(
							block.header.number().saturating_sub(One::one())
						);
						if let Some(data) = logs(client.as_ref(), number) {
							aux_schema::write_logs(
								number,
								data,
								insert_closure!()
							);
						}
					}
				},
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

fn logs<B, BE, C>(
	client: &C,
	block_number: u32,
) -> Option<(H256, Vec<TransactionStatus>)> where 
	B: BlockT,
	BE: Backend<B>,
	BE::State: StateBackend<BlakeTwo256>,
	C: HeaderBackend<B> + StorageProvider<B,BE>,
{
	if let Ok(Some(header)) = client.header(BlockId::Number(block_number.into()))
	{
		if let Ok(ConsensusLog::EndBlock { block_hash, .. }) = find_frontier_log::<B>(&header) {
			if let Ok(Some(data)) = client.storage(
				&BlockId::Number(block_number.into()),
				&StorageKey(
					storage_prefix_build(b"Ethereum", b"CurrentTransactionStatuses")
				)
			) {
				let statuses: Vec<TransactionStatus> = Decode::decode(&mut &data.0[..]).unwrap();
				return Some((block_hash, statuses))
			}
		}
	}
	None
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}
