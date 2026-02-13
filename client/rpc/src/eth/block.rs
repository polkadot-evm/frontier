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

use ethereum_types::{H256, U256};
use jsonrpsee::core::RpcResult;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	eth::{rich_block_build, BlockInfo, Eth},
	internal_err,
};

impl<B, C, P, CT, BE, CIDP, EC> Eth<B, C, P, CT, BE, CIDP, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	P: TransactionPool<Block = B, Hash = B::Hash> + 'static,
{
	pub async fn block_by_hash(&self, hash: H256, full: bool) -> RpcResult<Option<RichBlock>> {
		let BlockInfo {
			block,
			statuses,
			substrate_hash,
			base_fee,
			..
		} = self.block_info_by_eth_block_hash(hash).await?;

		match (block, statuses) {
			(Some(block), statuses) => {
				let statuses = statuses
					.map(|statuses| statuses.into_iter().map(Option::Some).collect())
					.unwrap_or_else(|| vec![None; block.transactions.len()]);
				let mut rich_block =
					rich_block_build(block, statuses, Some(hash), full, Some(base_fee), false);

				let substrate_hash = H256::from_slice(substrate_hash.as_ref());
				if let Some(parent_hash) = self
					.forced_parent_hashes
					.as_ref()
					.and_then(|parent_hashes| parent_hashes.get(&substrate_hash).cloned())
				{
					rich_block.inner.header.parent_hash = parent_hash
				}

				Ok(Some(rich_block))
			}
			_ => Ok(None),
		}
	}

	pub async fn block_by_number(
		&self,
		number_or_hash: BlockNumberOrHash,
		full: bool,
	) -> RpcResult<Option<RichBlock>> {
		// Handle pending blocks specially - they're not in mapping-sync
		if number_or_hash == BlockNumberOrHash::Pending {
			return self.pending_block(full).await;
		}

		// For all other block queries, use mapping-sync via block_info_by_number
		let BlockInfo {
			block,
			statuses,
			substrate_hash,
			base_fee,
			..
		} = self.block_info_by_number(number_or_hash).await?;

		match (block, statuses) {
			(Some(block), statuses) => {
				let statuses = statuses
					.map(|statuses| statuses.into_iter().map(Option::Some).collect())
					.unwrap_or_else(|| vec![None; block.transactions.len()]);
				let hash = H256::from(keccak_256(&rlp::encode(&block.header)));
				let mut rich_block =
					rich_block_build(block, statuses, Some(hash), full, Some(base_fee), false);

				let substrate_hash = H256::from_slice(substrate_hash.as_ref());
				if let Some(parent_hash) = self
					.forced_parent_hashes
					.as_ref()
					.and_then(|parent_hashes| parent_hashes.get(&substrate_hash).cloned())
				{
					rich_block.inner.header.parent_hash = parent_hash
				}

				Ok(Some(rich_block))
			}
			_ => Ok(None),
		}
	}

	async fn pending_block(&self, full: bool) -> RpcResult<Option<RichBlock>> {
		let api = self.client.runtime_api();
		let best_hash = self.client.info().best_hash;

		// Get current in-pool transactions
		let mut xts: Vec<<B as BlockT>::Extrinsic> = Vec::new();
		// ready validated pool
		xts.extend(
			self.pool
				.ready()
				.map(|in_pool_tx| in_pool_tx.data().as_ref().clone())
				.collect::<Vec<<B as BlockT>::Extrinsic>>(),
		);

		// future validated pool
		xts.extend(
			self.pool
				.futures()
				.iter()
				.map(|in_pool_tx| in_pool_tx.data().as_ref().clone())
				.collect::<Vec<<B as BlockT>::Extrinsic>>(),
		);

		let (block, statuses) = api
			.pending_block(best_hash, xts)
			.map_err(|_| internal_err(format!("Runtime access error at {best_hash}")))?;

		let base_fee = api.gas_price(best_hash).ok();

		match (block, statuses) {
			(Some(block), Some(statuses)) => Ok(Some(rich_block_build(
				block,
				statuses.into_iter().map(Option::Some).collect(),
				None,
				full,
				base_fee,
				true,
			))),
			_ => Ok(None),
		}
	}

	pub async fn block_transaction_count_by_hash(&self, hash: H256) -> RpcResult<Option<U256>> {
		let blockinfo = self.block_info_by_eth_block_hash(hash).await?;
		match blockinfo.block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	pub async fn block_transaction_count_by_number(
		&self,
		number_or_hash: BlockNumberOrHash,
	) -> RpcResult<Option<U256>> {
		if let BlockNumberOrHash::Pending = number_or_hash {
			// get the pending transactions count
			return Ok(Some(U256::from(self.pool.ready().count())));
		}

		let block_info = self.block_info_by_number(number_or_hash).await?;
		match block_info.block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	pub async fn block_transaction_receipts(
		&self,
		number_or_hash: BlockNumberOrHash,
	) -> RpcResult<Option<Vec<Receipt>>> {
		let block_info = self.block_info_by_number(number_or_hash).await?;
		let Some(statuses) = block_info.clone().statuses else {
			return Ok(None);
		};

		let mut receipts = Vec::new();
		let transactions: Vec<(H256, usize)> = statuses
			.iter()
			.map(|tx| (tx.transaction_hash, tx.transaction_index as usize))
			.collect();
		for (hash, index) in transactions {
			if let Some(receipt) = self.transaction_receipt(&block_info, hash, index).await? {
				receipts.push(receipt);
			}
		}

		Ok(Some(receipts))
	}

	pub fn block_uncles_count_by_hash(&self, _: H256) -> RpcResult<U256> {
		Ok(U256::zero())
	}

	pub fn block_uncles_count_by_number(&self, _: BlockNumberOrHash) -> RpcResult<U256> {
		Ok(U256::zero())
	}

	pub fn uncle_by_block_hash_and_index(&self, _: H256, _: Index) -> RpcResult<Option<RichBlock>> {
		Ok(None)
	}

	pub fn uncle_by_block_number_and_index(
		&self,
		_: BlockNumberOrHash,
		_: Index,
	) -> RpcResult<Option<RichBlock>> {
		Ok(None)
	}
}
