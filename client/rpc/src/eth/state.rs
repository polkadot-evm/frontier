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

use ethereum_types::{H160, H256, U256};
use jsonrpsee::core::RpcResult as Result;
use scale_codec::Encode;
// Substrate
use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sc_network_common::ExHashT;
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT},
};
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	eth::{pending_runtime_api, Eth},
	frontier_backend_client, internal_err,
};

impl<B, C, P, CT, BE, H: ExHashT, A: ChainApi, EGA> Eth<B, C, P, CT, BE, H, A, EGA>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	P: TransactionPool<Block = B> + Send + Sync + 'static,
	A: ChainApi<Block = B> + 'static,
{
	pub fn balance(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		let number = number.unwrap_or(BlockNumber::Latest);
		if number == BlockNumber::Pending {
			let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
			Ok(api
				.account_basic(&BlockId::Hash(self.client.info().best_hash), address)
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.balance)
		} else if let Ok(Some(id)) = frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		) {
			Ok(self
				.client
				.runtime_api()
				.account_basic(&id, address)
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.balance)
		} else {
			Ok(U256::zero())
		}
	}

	pub fn storage_at(
		&self,
		address: H160,
		index: U256,
		number: Option<BlockNumber>,
	) -> Result<H256> {
		let number = number.unwrap_or(BlockNumber::Latest);
		if number == BlockNumber::Pending {
			let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
			Ok(api
				.storage_at(&BlockId::Hash(self.client.info().best_hash), address, index)
				.unwrap_or_default())
		} else if let Ok(Some(id)) = frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		) {
			let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
				self.client.as_ref(),
				id,
			);
			Ok(self
				.overrides
				.schemas
				.get(&schema)
				.unwrap_or(&self.overrides.fallback)
				.storage_at(&id, address, index)
				.unwrap_or_default())
		} else {
			Ok(H256::default())
		}
	}

	pub fn transaction_count(&self, address: H160, number: Option<BlockNumber>) -> Result<U256> {
		if let Some(BlockNumber::Pending) = number {
			let block = BlockId::Hash(self.client.info().best_hash);

			let nonce = self
				.client
				.runtime_api()
				.account_basic(&block, address)
				.map_err(|err| {
					internal_err(format!("fetch runtime account basic failed: {:?}", err))
				})?
				.nonce;

			let mut current_nonce = nonce;
			let mut current_tag = (address, nonce).encode();
			for tx in self.pool.ready() {
				// since transactions in `ready()` need to be ordered by nonce
				// it's fine to continue with current iterator.
				if tx.provides().get(0) == Some(&current_tag) {
					current_nonce = current_nonce.saturating_add(1.into());
					current_tag = (address, current_nonce).encode();
				}
			}

			return Ok(current_nonce);
		}

		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number,
		)? {
			Some(id) => id,
			None => return Ok(U256::zero()),
		};

		Ok(self
			.client
			.runtime_api()
			.account_basic(&id, address)
			.map_err(|err| internal_err(format!("fetch runtime account basic failed: {:?}", err)))?
			.nonce)
	}

	pub fn code_at(&self, address: H160, number: Option<BlockNumber>) -> Result<Bytes> {
		let number = number.unwrap_or(BlockNumber::Latest);
		if number == BlockNumber::Pending {
			let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
			Ok(api
				.account_code_at(&BlockId::Hash(self.client.info().best_hash), address)
				.unwrap_or_default()
				.into())
		} else if let Ok(Some(id)) = frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		) {
			let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
				self.client.as_ref(),
				id,
			);

			Ok(self
				.overrides
				.schemas
				.get(&schema)
				.unwrap_or(&self.overrides.fallback)
				.account_code_at(&id, address)
				.unwrap_or_default()
				.into())
		} else {
			Ok(Bytes(vec![]))
		}
	}
}
