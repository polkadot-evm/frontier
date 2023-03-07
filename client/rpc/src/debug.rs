// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2023 Parity Technologies (UK) Ltd.
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

use std::{marker::PhantomData, sync::Arc};

use ethereum_types::H256;
use jsonrpsee::core::{async_trait, RpcResult as Result};
use rlp::Encodable;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::{types::*, DebugApiServer};
// use fc_storage::OverrideHandle;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{cache::EthBlockDataCacheTask, frontier_backend_client, internal_err};

/// Debug API implementation.
pub struct Debug<B: BlockT, C, BE> {
	client: Arc<C>,
	// overrides: Arc<OverrideHandle<B>>,
	backend: Arc<fc_db::Backend<B>>,
	block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, C, BE> Debug<B, C, BE> {
	pub fn new(
		client: Arc<C>,
		// overrides: Arc<OverrideHandle<B>>,
		backend: Arc<fc_db::Backend<B>>,
		block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	) -> Self {
		Self {
			client,
			// overrides,
			backend,
			block_data_cache,
			_marker: PhantomData,
		}
	}

	async fn block_by(&self, number: BlockNumber) -> Result<Option<ethereum::BlockV2>>
	where
		C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
		BE: Backend<B>,
	{
		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		)? {
			Some(id) => id,
			None => return Ok(None),
		};

		let substrate_hash = self
			.client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;
		let schema = fc_storage::onchain_storage_schema(self.client.as_ref(), substrate_hash);

		Ok(self
			.block_data_cache
			.current_block(schema, substrate_hash)
			.await)
	}
}

#[async_trait]
impl<B, C, BE> DebugApiServer for Debug<B, C, BE>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
{
	async fn raw_header(&self, number: BlockNumber) -> Result<Option<Bytes>> {
		let block = self.block_by(number).await?;
		Ok(block.map(|block| Bytes::new(block.header.rlp_bytes().to_vec())))
	}

	async fn raw_block(&self, number: BlockNumber) -> Result<Option<Bytes>> {
		let block = self.block_by(number).await?;
		Ok(block.map(|block| Bytes::new(block.rlp_bytes().to_vec())))
	}

	async fn raw_transaction(&self, _hash: H256) -> Result<Option<Bytes>> {
		todo!()
	}

	async fn raw_receipts(&self, _number: BlockNumber) -> Result<Vec<Bytes>> {
		todo!()
	}

	fn bad_blocks(&self, _number: BlockNumber) -> Result<Vec<()>> {
		// `debug_getBadBlocks` wouldn't really be useful in a Substrate context.
		// The rationale for that is for debugging multi-client consensus issues, which we'll never face
		// (we may have multiple clients in the future, but for runtime it's only "multi-wasm-runtime", never "multi-EVM").
		// We can simply return empty array for this API.
		Ok(vec![])
	}
}
