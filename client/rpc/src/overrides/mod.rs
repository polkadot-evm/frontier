// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2017-2022 Parity Technologies (UK) Ltd.
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

use ethereum_types::{H160, H256, U256};
// Substrate
use sp_api::{ApiExt, BlockId, ProvideRuntimeApi};
use sp_io::hashing::{blake2_128, twox_128};
use sp_runtime::{traits::Block as BlockT, Permill};
// Frontier
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};
pub use fp_storage::{EthereumStorageSchema, OverrideHandle, StorageOverride};

mod schema_v1_override;
mod schema_v2_override;
mod schema_v3_override;

pub use schema_v1_override::SchemaV1Override;
pub use schema_v2_override::SchemaV2Override;
pub use schema_v3_override::SchemaV3Override;

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn blake2_128_extend(bytes: &[u8]) -> Vec<u8> {
	let mut ext: Vec<u8> = blake2_128(bytes).to_vec();
	ext.extend_from_slice(bytes);
	ext
}

/// A wrapper type for the Runtime API. This type implements `StorageOverride`, so it can be used
/// when calling the runtime API is desired but a `dyn StorageOverride` is required.
pub struct RuntimeApiStorageOverride<B: BlockT, C> {
	client: Arc<C>,
	_marker: PhantomData<B>,
}

impl<B: BlockT, C> RuntimeApiStorageOverride<B, C> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<Block, C> StorageOverride<Block> for RuntimeApiStorageOverride<Block, C>
where
	Block: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<Block>,
{
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<Block>, address: H160) -> Option<Vec<u8>> {
		self.client
			.runtime_api()
			.account_code_at(block, address)
			.ok()
	}

	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<Block>, address: H160, index: U256) -> Option<H256> {
		self.client
			.runtime_api()
			.storage_at(block, address, index)
			.ok()
	}

	/// Return the current block.
	fn current_block(&self, block: &BlockId<Block>) -> Option<ethereum::BlockV2> {
		let api = self.client.runtime_api();

		let api_version = if let Ok(Some(api_version)) =
			api.api_version::<dyn EthereumRuntimeRPCApi<Block>>(block)
		{
			api_version
		} else {
			return None;
		};
		if api_version == 1 {
			#[allow(deprecated)]
			let old_block = api.current_block_before_version_2(block).ok()?;
			old_block.map(|block| block.into())
		} else {
			api.current_block(block).ok()?
		}
	}

	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<Block>) -> Option<Vec<ethereum::ReceiptV3>> {
		let api = self.client.runtime_api();

		let api_version = if let Ok(Some(api_version)) =
			api.api_version::<dyn EthereumRuntimeRPCApi<Block>>(block)
		{
			api_version
		} else {
			return None;
		};
		if api_version < 4 {
			#[allow(deprecated)]
			let old_receipts = api.current_receipts_before_version_4(block).ok()?;
			old_receipts.map(|receipts| {
				receipts
					.into_iter()
					.map(|r| {
						ethereum::ReceiptV3::Legacy(ethereum::EIP658ReceiptData {
							status_code: r.state_root.to_low_u64_be() as u8,
							used_gas: r.used_gas,
							logs_bloom: r.logs_bloom,
							logs: r.logs,
						})
					})
					.collect()
			})
		} else {
			self.client.runtime_api().current_receipts(block).ok()?
		}
	}

	/// Return the current transaction status.
	fn current_transaction_statuses(
		&self,
		block: &BlockId<Block>,
	) -> Option<Vec<TransactionStatus>> {
		self.client
			.runtime_api()
			.current_transaction_statuses(block)
			.ok()?
	}

	/// Return the elasticity multiplier at the give post-eip1559 height.
	fn elasticity(&self, block: &BlockId<Block>) -> Option<Permill> {
		if self.is_eip1559(block) {
			self.client.runtime_api().elasticity(block).ok()?
		} else {
			None
		}
	}

	fn is_eip1559(&self, block: &BlockId<Block>) -> bool {
		if let Ok(Some(api_version)) = self
			.client
			.runtime_api()
			.api_version::<dyn EthereumRuntimeRPCApi<Block>>(block)
		{
			return api_version >= 2;
		}
		false
	}
}
