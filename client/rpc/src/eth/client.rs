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

use ethereum_types::{H160, H256, U256, U64};
use jsonrpc_core::Result;

use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sc_network::ExHashT;
use sc_transaction_pool::ChainApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT, UniqueSaturatedInto},
};

use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{eth::EthApi, frontier_backend_client, internal_err};

impl<B, C, P, CT, BE, H: ExHashT, A: ChainApi> EthApi<B, C, P, CT, BE, H, A>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	pub fn protocol_version(&self) -> Result<u64> {
		Ok(1)
	}

	pub fn syncing(&self) -> Result<SyncStatus> {
		if self.network.is_major_syncing() {
			let block_number = U256::from(UniqueSaturatedInto::<u128>::unique_saturated_into(
				self.client.info().best_number.clone(),
			));
			Ok(SyncStatus::Info(SyncInfo {
				starting_block: U256::zero(),
				current_block: block_number,
				// TODO `highest_block` is not correct, should load `best_seen_block` from NetworkWorker,
				// but afaik that is not currently possible in Substrate:
				// https://github.com/paritytech/substrate/issues/7311
				highest_block: block_number,
				warp_chunks_amount: None,
				warp_chunks_processed: None,
			}))
		} else {
			Ok(SyncStatus::None)
		}
	}

	pub fn author(&self) -> Result<H160> {
		let block = BlockId::Hash(self.client.info().best_hash);
		let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
			self.client.as_ref(),
			block,
		);

		Ok(self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(&block)
			.ok_or(internal_err("fetching author through override failed"))?
			.header
			.beneficiary)
	}

	pub fn accounts(&self) -> Result<Vec<H160>> {
		let mut accounts = Vec::new();
		for signer in &*self.signers {
			accounts.append(&mut signer.accounts());
		}
		Ok(accounts)
	}

	pub fn block_number(&self) -> Result<U256> {
		Ok(U256::from(
			UniqueSaturatedInto::<u128>::unique_saturated_into(
				self.client.info().best_number.clone(),
			),
		))
	}

	pub fn chain_id(&self) -> Result<Option<U64>> {
		let hash = self.client.info().best_hash;
		Ok(Some(
			self.client
				.runtime_api()
				.chain_id(&BlockId::Hash(hash))
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.into(),
		))
	}
}
