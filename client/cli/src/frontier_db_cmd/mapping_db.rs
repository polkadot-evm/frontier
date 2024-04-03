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

use std::sync::Arc;

use ethereum_types::H256;
use serde::Deserialize;
// Substrate
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fp_rpc::EthereumRuntimeRPCApi;

use super::{utils::FrontierDbMessage, Column, FrontierDbCmd, Operation};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MappingValue<H> {
	SubstrateBlockHash(H),
}

#[derive(Clone, Copy, Debug)]
pub enum MappingKey {
	EthBlockOrTransactionHash(H256),
}

pub struct MappingDb<'a, C, B: BlockT> {
	cmd: &'a FrontierDbCmd,
	client: Arc<C>,
	backend: Arc<fc_db::kv::Backend<B>>,
}

impl<'a, C, B: BlockT> MappingDb<'a, C, B>
where
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B>,
{
	pub fn new(
		cmd: &'a FrontierDbCmd,
		client: Arc<C>,
		backend: Arc<fc_db::kv::Backend<B>>,
	) -> Self {
		Self {
			cmd,
			client,
			backend,
		}
	}

	pub fn query(
		&self,
		column: &Column,
		key: &MappingKey,
		value: &Option<MappingValue<B::Hash>>,
	) -> sc_cli::Result<()> {
		match self.cmd.operation {
			Operation::Create => match (key, value) {
				// Insert a mapping commitment using the state at the requested block.
				(
					MappingKey::EthBlockOrTransactionHash(ethereum_block_hash),
					Some(MappingValue::SubstrateBlockHash(substrate_block_hash)),
				) => {
					if self
						.backend
						.mapping()
						.block_hash(ethereum_block_hash)?
						.is_none()
					{
						let existing_transaction_hashes: Vec<H256> = if let Some(statuses) = self
							.client
							.runtime_api()
							.current_transaction_statuses(*substrate_block_hash)
							.map_err(|e| format!("{:?}", e))?
						{
							statuses
								.iter()
								.map(|t| t.transaction_hash)
								.collect::<Vec<H256>>()
						} else {
							vec![]
						};

						let commitment = fc_db::kv::MappingCommitment::<B> {
							block_hash: *substrate_block_hash,
							ethereum_block_hash: *ethereum_block_hash,
							ethereum_transaction_hashes: existing_transaction_hashes,
						};

						self.backend.mapping().write_hashes(commitment)?;
					} else {
						return Err(self.key_not_empty_error(key));
					}
				}
				_ => return Err(self.key_value_error(key, value)),
			},
			Operation::Read => match (column, key) {
				// Given ethereum block hash, get substrate block hash.
				(Column::Block, MappingKey::EthBlockOrTransactionHash(ethereum_block_hash)) => {
					let value = self.backend.mapping().block_hash(ethereum_block_hash)?;
					println!("{:?}", value);
				}
				// Given ethereum transaction hash, get transaction metadata.
				(
					Column::Transaction,
					MappingKey::EthBlockOrTransactionHash(ethereum_transaction_hash),
				) => {
					let value = self
						.backend
						.mapping()
						.transaction_metadata(ethereum_transaction_hash)?;
					println!("{:?}", value);
				}
				_ => return Err(self.key_column_error(key, value)),
			},
			Operation::Update => match (key, value) {
				// Update a mapping commitment using the state at the requested block.
				(
					MappingKey::EthBlockOrTransactionHash(ethereum_block_hash),
					Some(MappingValue::SubstrateBlockHash(substrate_block_hash)),
				) => {
					if self
						.backend
						.mapping()
						.block_hash(ethereum_block_hash)?
						.is_some()
					{
						let existing_transaction_hashes: Vec<H256> = if let Some(statuses) = self
							.client
							.runtime_api()
							.current_transaction_statuses(*substrate_block_hash)
							.map_err(|e| format!("{:?}", e))?
						{
							statuses
								.iter()
								.map(|t| t.transaction_hash)
								.collect::<Vec<H256>>()
						} else {
							vec![]
						};

						let commitment = fc_db::kv::MappingCommitment::<B> {
							block_hash: *substrate_block_hash,
							ethereum_block_hash: *ethereum_block_hash,
							ethereum_transaction_hashes: existing_transaction_hashes,
						};

						self.backend.mapping().write_hashes(commitment)?;
					}
				}
				_ => return Err(self.key_value_error(key, value)),
			},
			Operation::Delete => {
				return Err("Delete operation is not supported for non-static keys"
					.to_string()
					.into())
			}
		}
		Ok(())
	}
}

impl<'a, C, B: BlockT> FrontierDbMessage for MappingDb<'a, C, B> {}
