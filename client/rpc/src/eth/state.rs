// This file is part of Tokfin.

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

use ethereum_types::{H160, H256, U256};
use jsonrpsee::core::RpcResult;
use scale_codec::Encode;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::traits::Block as BlockT;
// Tokfin
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{eth::Eth, tokfin_backend_client, internal_err};

impl<B, C, P, CT, BE, CIDP, EC> Eth<B, C, P, CT, BE, CIDP, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	P: TransactionPool<Block = B, Hash = B::Hash> + 'static,
	CIDP: CreateInherentDataProviders<B, ()> + Send + 'static,
{
	pub async fn balance(
		&self,
		address: H160,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<U256> {
		let number_or_hash = number_or_hash.unwrap_or(BlockNumberOrHash::Latest);
		if number_or_hash == BlockNumberOrHash::Pending {
			let (hash, api) = self
				.pending_runtime_api()
				.await
				.map_err(|err| internal_err(format!("Create pending runtime api error: {err}")))?;
			Ok(api
				.account_basic(hash, address)
				.map_err(|err| internal_err(format!("Fetch account balances failed: {err}")))?
				.balance)
		} else if let Ok(Some(id)) = tokfin_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number_or_hash),
		)
		.await
		{
			let substrate_hash = self
				.client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {id}")))?;

			Ok(self
				.client
				.runtime_api()
				.account_basic(substrate_hash, address)
				.map_err(|err| internal_err(format!("Fetch account balances failed: {:?}", err)))?
				.balance)
		} else {
			Ok(U256::zero())
		}
	}

	pub async fn storage_at(
		&self,
		address: H160,
		index: U256,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<H256> {
		let number_or_hash = number_or_hash.unwrap_or(BlockNumberOrHash::Latest);
		if number_or_hash == BlockNumberOrHash::Pending {
			let (hash, api) = self
				.pending_runtime_api()
				.await
				.map_err(|err| internal_err(format!("Create pending runtime api error: {err}")))?;
			Ok(api.storage_at(hash, address, index).unwrap_or_default())
		} else if let Ok(Some(id)) = tokfin_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number_or_hash),
		)
		.await
		{
			let substrate_hash = self
				.client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {id}")))?;
			Ok(self
				.storage_override
				.account_storage_at(substrate_hash, address, index)
				.unwrap_or_default())
		} else {
			Ok(H256::default())
		}
	}

	pub async fn transaction_count(
		&self,
		address: H160,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<U256> {
		if let Some(BlockNumberOrHash::Pending) = number_or_hash {
			let substrate_hash = self.client.info().best_hash;

			let nonce = self
				.client
				.runtime_api()
				.account_basic(substrate_hash, address)
				.map_err(|err| internal_err(format!("Fetch account nonce failed: {err}")))?
				.nonce;

			let mut current_nonce = nonce;
			let mut current_tag = (address, nonce).encode();
			for tx in self.pool.ready() {
				// since transactions in `ready()` need to be ordered by nonce
				// it's fine to continue with current iterator.
				if tx.provides().first() == Some(&current_tag) {
					current_nonce = current_nonce.saturating_add(1.into());
					current_tag = (address, current_nonce).encode();
				}
			}

			return Ok(current_nonce);
		}

		let id = match tokfin_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number_or_hash,
		)
		.await?
		{
			Some(id) => id,
			None => return Ok(U256::zero()),
		};

		let substrate_hash = self
			.client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {id}")))?;

		Ok(self
			.client
			.runtime_api()
			.account_basic(substrate_hash, address)
			.map_err(|err| internal_err(format!("Fetch account nonce failed: {err}")))?
			.nonce)
	}

	pub async fn code_at(
		&self,
		address: H160,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<Bytes> {
		let number_or_hash = number_or_hash.unwrap_or(BlockNumberOrHash::Latest);
		if number_or_hash == BlockNumberOrHash::Pending {
			let (hash, api) = self
				.pending_runtime_api()
				.await
				.map_err(|err| internal_err(format!("Create pending runtime api error: {err}")))?;
			Ok(api
				.account_code_at(hash, address)
				.unwrap_or_default()
				.into())
		} else if let Ok(Some(id)) = tokfin_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number_or_hash),
		)
		.await
		{
			let substrate_hash = self
				.client
				.expect_block_hash_from_id(&id)
				.map_err(|_| internal_err(format!("Expect block number from id: {id}")))?;
			Ok(self
				.storage_override
				.account_code_at(substrate_hash, address)
				.unwrap_or_default()
				.into())
		} else {
			Ok(Bytes(vec![]))
		}
	}
}
