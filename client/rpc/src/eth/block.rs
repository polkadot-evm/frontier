// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2022 Parity Technologies (UK) Ltd.
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

use ethereum_types::{H256, U256};
use jsonrpc_core::{BoxFuture, Result};

use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sc_network::ExHashT;
use sc_transaction_pool::ChainApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};

use fc_rpc_core::types::*;

use crate::{
	eth::{rich_block_build, EthApi},
	frontier_backend_client, internal_err,
};

impl<B, C, P, CT, BE, H: ExHashT, A: ChainApi> EthApi<B, C, P, CT, BE, H, A>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: StorageProvider<B, BE> + HeaderBackend<B> + Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	pub fn block_by_hash(&self, hash: H256, full: bool) -> BoxFuture<Result<Option<RichBlock>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		Box::pin(async move {
			let id = match frontier_backend_client::load_hash::<B>(backend.as_ref(), hash)
				.map_err(|err| internal_err(format!("{:?}", err)))?
			{
				Some(hash) => hash,
				_ => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;

			let base_fee = handler.base_fee(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses) {
				(Some(block), Some(statuses)) => Ok(Some(rich_block_build(
					block,
					statuses.into_iter().map(|s| Some(s)).collect(),
					Some(hash),
					full,
					base_fee,
					is_eip1559,
				))),
				_ => Ok(None),
			}
		})
	}

	pub fn block_by_number(
		&self,
		number: BlockNumber,
		full: bool,
	) -> BoxFuture<Result<Option<RichBlock>>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		Box::pin(async move {
			let id = match frontier_backend_client::native_block_id::<B, C>(
				client.as_ref(),
				backend.as_ref(),
				Some(number),
			)? {
				Some(id) => id,
				None => return Ok(None),
			};
			let substrate_hash = client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

			let schema =
				frontier_backend_client::onchain_storage_schema::<B, C, BE>(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let block = block_data_cache.current_block(schema, substrate_hash).await;
			let statuses = block_data_cache
				.current_transaction_statuses(schema, substrate_hash)
				.await;

			let base_fee = handler.base_fee(&id);
			let is_eip1559 = handler.is_eip1559(&id);

			match (block, statuses) {
				(Some(block), Some(statuses)) => {
					let hash = H256::from(keccak_256(&rlp::encode(&block.header)));

					Ok(Some(rich_block_build(
						block,
						statuses.into_iter().map(|s| Some(s)).collect(),
						Some(hash),
						full,
						base_fee,
						is_eip1559,
					)))
				}
				_ => Ok(None),
			}
		})
	}

	pub fn block_transaction_count_by_hash(&self, hash: H256) -> Result<Option<U256>> {
		let id = match frontier_backend_client::load_hash::<B>(self.backend.as_ref(), hash)
			.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let schema =
			frontier_backend_client::onchain_storage_schema::<B, C, BE>(self.client.as_ref(), id);
		let block = self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(&id);

		match block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	pub fn block_transaction_count_by_number(&self, number: BlockNumber) -> Result<Option<U256>> {
		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		)? {
			Some(id) => id,
			None => return Ok(None),
		};
		let schema =
			frontier_backend_client::onchain_storage_schema::<B, C, BE>(self.client.as_ref(), id);
		let block = self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(&id);

		match block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	pub fn block_uncles_count_by_hash(&self, _: H256) -> Result<U256> {
		Ok(U256::zero())
	}

	pub fn block_uncles_count_by_number(&self, _: BlockNumber) -> Result<U256> {
		Ok(U256::zero())
	}

	pub fn uncle_by_block_hash_and_index(&self, _: H256, _: Index) -> Result<Option<RichBlock>> {
		Ok(None)
	}

	pub fn uncle_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> Result<Option<RichBlock>> {
		Ok(None)
	}
}
