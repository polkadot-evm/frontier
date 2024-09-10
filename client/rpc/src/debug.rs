// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use std::{marker::PhantomData, sync::Arc};

use ethereum::EnvelopedEncodable;
use ethereum_types::H256;
use jsonrpsee::core::{async_trait, RpcResult};
use rlp::Encodable;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::{types::*, DebugApiServer};
use fc_storage::StorageOverride;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{cache::EthBlockDataCacheTask, frontier_backend_client, internal_err};

/// Debug API implementation.
pub struct Debug<B: BlockT, C, BE> {
	client: Arc<C>,
	backend: Arc<dyn fc_api::Backend<B>>,
	storage_override: Arc<dyn StorageOverride<B>>,
	block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, C, BE> Debug<B, C, BE> {
	pub fn new(
		client: Arc<C>,
		backend: Arc<dyn fc_api::Backend<B>>,
		storage_override: Arc<dyn StorageOverride<B>>,
		block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	) -> Self {
		Self {
			client,
			backend,
			storage_override,
			block_data_cache,
			_marker: PhantomData,
		}
	}

	async fn block_by(&self, number: BlockNumberOrHash) -> RpcResult<Option<ethereum::BlockV2>>
	where
		C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
		BE: Backend<B>,
	{
		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		)
		.await?
		{
			Some(id) => id,
			None => return Ok(None),
		};

		let substrate_hash = self
			.client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;
		let block = self.block_data_cache.current_block(substrate_hash).await;
		Ok(block)
	}

	async fn transaction_by(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<ethereum::TransactionV2>>
	where
		C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
		BE: Backend<B>,
	{
		let (eth_block_hash, index) = match frontier_backend_client::load_transactions::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			transaction_hash,
			true,
		)
		.await?
		{
			Some((hash, index)) => (hash, index as usize),
			None => return Ok(None),
		};

		let substrate_hash = match frontier_backend_client::load_hash::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			eth_block_hash,
		)
		.await?
		{
			Some(hash) => hash,
			None => return Ok(None),
		};

		let block = self.block_data_cache.current_block(substrate_hash).await;
		if let Some(block) = block {
			Ok(Some(block.transactions[index].clone()))
		} else {
			Ok(None)
		}
	}

	async fn receipts_by(
		&self,
		number: BlockNumberOrHash,
	) -> RpcResult<Option<Vec<ethereum::ReceiptV3>>>
	where
		C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
		BE: Backend<B>,
	{
		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		)
		.await?
		{
			Some(id) => id,
			None => return Ok(None),
		};

		let substrate_hash = self
			.client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

		// TODO: use data cache in the future
		let receipts = self.storage_override.current_receipts(substrate_hash);
		Ok(receipts)
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
	async fn raw_header(&self, number: BlockNumberOrHash) -> RpcResult<Option<Bytes>> {
		let block = self.block_by(number).await?;
		Ok(block.map(|block| Bytes::new(block.header.rlp_bytes().to_vec())))
	}

	async fn raw_block(&self, number: BlockNumberOrHash) -> RpcResult<Option<Bytes>> {
		let block = self.block_by(number).await?;
		Ok(block.map(|block| Bytes::new(block.rlp_bytes().to_vec())))
	}

	async fn raw_transaction(&self, hash: H256) -> RpcResult<Option<Bytes>> {
		let transaction = self.transaction_by(hash).await?;
		Ok(transaction.map(|transaction| Bytes::new(transaction.encode().to_vec())))
	}

	async fn raw_receipts(&self, number: BlockNumberOrHash) -> RpcResult<Vec<Bytes>> {
		let receipts = self.receipts_by(number).await?.unwrap_or_default();
		Ok(receipts
			.into_iter()
			.map(|receipt| Bytes::new(receipt.encode().to_vec()))
			.collect::<Vec<_>>())
	}

	fn bad_blocks(&self, _number: BlockNumberOrHash) -> RpcResult<Vec<()>> {
		// `debug_getBadBlocks` wouldn't really be useful in a Substrate context.
		// The rationale for that is for debugging multi-client consensus issues, which we'll never face
		// (we may have multiple clients in the future, but for runtime it's only "multi-wasm-runtime", never "multi-EVM").
		// We can simply return empty array for this API.
		Ok(vec![])
	}
}
