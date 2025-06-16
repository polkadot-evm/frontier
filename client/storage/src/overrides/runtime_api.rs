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

use ethereum_types::{Address, H256, U256};
// Substrate
use sp_api::{ApiExt, ApiRef, ProvideRuntimeApi};
use sp_runtime::{traits::Block as BlockT, Permill};
// Frontier
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};

use crate::overrides::StorageOverride;

/// A storage override for runtimes that use runtime API.
#[derive(Clone)]
pub struct RuntimeApiStorageOverride<B, C> {
	client: Arc<C>,
	_marker: PhantomData<B>,
}

impl<B, C> RuntimeApiStorageOverride<B, C> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<B, C> RuntimeApiStorageOverride<B, C>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
{
	fn api_version(api: &ApiRef<'_, C::Api>, block_hash: B::Hash) -> Option<u32> {
		match api.api_version::<dyn EthereumRuntimeRPCApi<B>>(block_hash) {
			Ok(Some(api_version)) => Some(api_version),
			_ => None,
		}
	}
}

impl<B, C> StorageOverride<B> for RuntimeApiStorageOverride<B, C>
where
	B: BlockT,
	C: ProvideRuntimeApi<B> + Send + Sync,
	C::Api: EthereumRuntimeRPCApi<B>,
{
	fn account_code_at(&self, block_hash: B::Hash, address: Address) -> Option<Vec<u8>> {
		self.client
			.runtime_api()
			.account_code_at(block_hash, address)
			.ok()
	}

	fn account_storage_at(
		&self,
		block_hash: B::Hash,
		address: Address,
		index: U256,
	) -> Option<H256> {
		self.client
			.runtime_api()
			.storage_at(block_hash, address, index)
			.ok()
	}

	fn current_block(&self, block_hash: B::Hash) -> Option<ethereum::BlockV3> {
		let api = self.client.runtime_api();

		let api_version = Self::api_version(&api, block_hash)?;
		if api_version == 1 {
			#[allow(deprecated)]
			let old_block = api.current_block_before_version_2(block_hash).ok()?;
			old_block.map(|block| block.into())
		} else {
			api.current_block(block_hash).ok()?
		}
	}

	fn current_receipts(&self, block_hash: B::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
		let api = self.client.runtime_api();

		let api_version = Self::api_version(&api, block_hash)?;
		if api_version < 4 {
			#[allow(deprecated)]
			let old_receipts = api.current_receipts_before_version_4(block_hash).ok()?;
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
			self.client
				.runtime_api()
				.current_receipts(block_hash)
				.ok()?
		}
	}

	fn current_transaction_statuses(&self, block_hash: B::Hash) -> Option<Vec<TransactionStatus>> {
		self.client
			.runtime_api()
			.current_transaction_statuses(block_hash)
			.ok()?
	}

	fn elasticity(&self, block_hash: B::Hash) -> Option<Permill> {
		if self.is_eip1559(block_hash) {
			self.client.runtime_api().elasticity(block_hash).ok()?
		} else {
			None
		}
	}

	fn is_eip1559(&self, block_hash: B::Hash) -> bool {
		let api = self.client.runtime_api();
		if let Some(api_version) = Self::api_version(&api, block_hash) {
			api_version >= 2
		} else {
			false
		}
	}
}
