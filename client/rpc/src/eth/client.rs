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

use ethereum_types::{H160, U256, U64};
use jsonrpsee::core::RpcResult;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool::ChainApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{eth::Eth, internal_err};

impl<B, C, P, CT, BE, A, CIDP, EC> Eth<B, C, P, CT, BE, A, CIDP, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B>,
	A: ChainApi<Block = B>,
{
	pub fn protocol_version(&self) -> RpcResult<u64> {
		Ok(1)
	}

	pub async fn syncing(&self) -> RpcResult<SyncStatus> {
		if self.sync.is_major_syncing() {
			let current_number = self.client.info().best_number;
			let highest_number = self
				.sync
				.best_seen_block()
				.await
				.map_err(|_| internal_err("fetch best_seen_block failed"))?
				.unwrap_or(current_number);

			let current_number = UniqueSaturatedInto::<u128>::unique_saturated_into(current_number);
			let highest_number = UniqueSaturatedInto::<u128>::unique_saturated_into(highest_number);

			Ok(SyncStatus::Info(SyncInfo {
				starting_block: U256::zero(),
				current_block: U256::from(current_number),
				highest_block: U256::from(highest_number),
				warp_chunks_amount: None,
				warp_chunks_processed: None,
			}))
		} else {
			Ok(SyncStatus::None)
		}
	}

	pub fn author(&self) -> RpcResult<H160> {
		let hash = self.client.info().best_hash;
		let schema = fc_storage::onchain_storage_schema(self.client.as_ref(), hash);
		let current_block = self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(hash)
			.ok_or_else(|| internal_err("fetching author through override failed"))?;
		Ok(current_block.header.beneficiary)
	}

	pub fn accounts(&self) -> RpcResult<Vec<H160>> {
		Ok(self
			.signers
			.iter()
			.flat_map(|signer| signer.accounts())
			.collect::<Vec<_>>())
	}

	pub fn block_number(&self) -> RpcResult<U256> {
		let best_number = self.client.info().best_number;
		let best_number = UniqueSaturatedInto::<u128>::unique_saturated_into(best_number);
		Ok(U256::from(best_number))
	}

	pub fn chain_id(&self) -> RpcResult<Option<U64>> {
		let hash = self.client.info().best_hash;
		let chain_id = self
			.client
			.runtime_api()
			.chain_id(hash)
			.map_err(|err| internal_err(format!("fetch runtime chain id failed: {err:?}")))?;
		Ok(Some(U64::from(chain_id)))
	}
}
