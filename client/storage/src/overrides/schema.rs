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

use std::sync::Arc;

use ethereum_types::{Address, H256, U256};
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sp_runtime::{traits::Block as BlockT, Permill};
// Frontier
use fp_rpc::TransactionStatus;

use crate::overrides::{StorageOverride, StorageQuerier};

pub mod v1 {
	use super::*;

	/// A storage override for runtimes that use schema v1.
	#[derive(Clone)]
	pub struct SchemaStorageOverride<B, C, BE> {
		querier: StorageQuerier<B, C, BE>,
	}

	impl<B, C, BE> SchemaStorageOverride<B, C, BE> {
		pub fn new(client: Arc<C>) -> Self {
			let querier = StorageQuerier::new(client);
			Self { querier }
		}
	}

	impl<B, C, BE> StorageOverride<B> for SchemaStorageOverride<B, C, BE>
	where
		B: BlockT,
		C: StorageProvider<B, BE> + Send + Sync,
		BE: Backend<B>,
	{
		fn account_code_at(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
			SchemaStorageOverrideRef::new(&self.querier).account_code_at(at, address)
		}

		fn account_storage_at(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
			SchemaStorageOverrideRef::new(&self.querier).account_storage_at(at, address, index)
		}

		fn current_block(&self, at: B::Hash) -> Option<ethereum::BlockV3> {
			SchemaStorageOverrideRef::new(&self.querier).current_block(at)
		}

		fn current_receipts(&self, at: B::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
			SchemaStorageOverrideRef::new(&self.querier).current_receipts(at)
		}

		fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
			SchemaStorageOverrideRef::new(&self.querier).current_transaction_statuses(at)
		}

		fn elasticity(&self, at: B::Hash) -> Option<Permill> {
			SchemaStorageOverrideRef::new(&self.querier).elasticity(at)
		}

		fn is_eip1559(&self, at: B::Hash) -> bool {
			SchemaStorageOverrideRef::new(&self.querier).is_eip1559(at)
		}
	}

	/// A storage override reference for runtimes that use schema v1.
	pub struct SchemaStorageOverrideRef<'a, B, C, BE> {
		querier: &'a StorageQuerier<B, C, BE>,
	}

	impl<'a, B, C, BE> SchemaStorageOverrideRef<'a, B, C, BE> {
		pub fn new(querier: &'a StorageQuerier<B, C, BE>) -> Self {
			Self { querier }
		}
	}

	impl<'a, B, C, BE> StorageOverride<B> for SchemaStorageOverrideRef<'a, B, C, BE>
	where
		B: BlockT,
		C: StorageProvider<B, BE> + Send + Sync,
		BE: Backend<B>,
	{
		fn account_code_at(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
			self.querier.account_code(at, address)
		}

		fn account_storage_at(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
			self.querier.account_storage(at, address, index)
		}

		fn current_block(&self, at: B::Hash) -> Option<ethereum::BlockV3> {
			self.querier
				.current_block::<ethereum::BlockV0>(at)
				.map(Into::into)
		}

		fn current_receipts(&self, at: B::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
			self.querier
				.current_receipts::<ethereum::ReceiptV0>(at)
				.map(|receipts| {
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
		}

		fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
			self.querier.current_transaction_statuses(at)
		}

		fn elasticity(&self, _at: B::Hash) -> Option<Permill> {
			None
		}

		fn is_eip1559(&self, _at: B::Hash) -> bool {
			false
		}
	}
}

pub mod v2 {
	use super::*;

	/// A storage override for runtimes that use schema v2.
	#[derive(Clone)]
	pub struct SchemaStorageOverride<B, C, BE> {
		querier: StorageQuerier<B, C, BE>,
	}

	impl<B, C, BE> SchemaStorageOverride<B, C, BE> {
		pub fn new(client: Arc<C>) -> Self {
			let querier = StorageQuerier::new(client);
			Self { querier }
		}
	}

	impl<B, C, BE> StorageOverride<B> for SchemaStorageOverride<B, C, BE>
	where
		B: BlockT,
		C: StorageProvider<B, BE> + Send + Sync,
		BE: Backend<B>,
	{
		fn account_code_at(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
			SchemaStorageOverrideRef::new(&self.querier).account_code_at(at, address)
		}

		fn account_storage_at(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
			SchemaStorageOverrideRef::new(&self.querier).account_storage_at(at, address, index)
		}

		fn current_block(&self, at: B::Hash) -> Option<ethereum::BlockV3> {
			SchemaStorageOverrideRef::new(&self.querier).current_block(at)
		}

		fn current_receipts(&self, at: B::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
			SchemaStorageOverrideRef::new(&self.querier).current_receipts(at)
		}

		fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
			SchemaStorageOverrideRef::new(&self.querier).current_transaction_statuses(at)
		}

		fn elasticity(&self, at: B::Hash) -> Option<Permill> {
			SchemaStorageOverrideRef::new(&self.querier).elasticity(at)
		}

		fn is_eip1559(&self, at: B::Hash) -> bool {
			SchemaStorageOverrideRef::new(&self.querier).is_eip1559(at)
		}
	}

	/// A storage override reference for runtimes that use schema v2.
	pub struct SchemaStorageOverrideRef<'a, B, C, BE> {
		querier: &'a StorageQuerier<B, C, BE>,
	}

	impl<'a, B, C, BE> SchemaStorageOverrideRef<'a, B, C, BE> {
		pub fn new(querier: &'a StorageQuerier<B, C, BE>) -> Self {
			Self { querier }
		}
	}

	impl<'a, B, C, BE> StorageOverride<B> for SchemaStorageOverrideRef<'a, B, C, BE>
	where
		B: BlockT,
		C: StorageProvider<B, BE> + Send + Sync,
		BE: Backend<B>,
	{
		fn account_code_at(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
			self.querier.account_code(at, address)
		}

		fn account_storage_at(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
			self.querier.account_storage(at, address, index)
		}

		fn current_block(&self, at: B::Hash) -> Option<ethereum::BlockV3> {
			self.querier.current_block(at)
		}

		fn current_receipts(&self, at: B::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
			self.querier
				.current_receipts::<ethereum::ReceiptV0>(at)
				.map(|receipts| {
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
		}

		fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
			self.querier.current_transaction_statuses(at)
		}

		fn elasticity(&self, at: B::Hash) -> Option<Permill> {
			self.querier.elasticity(at)
		}

		fn is_eip1559(&self, _at: B::Hash) -> bool {
			true
		}
	}
}

pub mod v3 {
	use super::*;

	/// A storage override for runtimes that use schema v3.
	#[derive(Clone)]
	pub struct SchemaStorageOverride<B, C, BE> {
		querier: StorageQuerier<B, C, BE>,
	}

	impl<B, C, BE> SchemaStorageOverride<B, C, BE> {
		pub fn new(client: Arc<C>) -> Self {
			let querier = StorageQuerier::new(client);
			Self { querier }
		}
	}

	impl<B, C, BE> StorageOverride<B> for SchemaStorageOverride<B, C, BE>
	where
		B: BlockT,
		C: StorageProvider<B, BE> + Send + Sync,
		BE: Backend<B>,
	{
		fn account_code_at(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
			SchemaStorageOverrideRef::new(&self.querier).account_code_at(at, address)
		}

		fn account_storage_at(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
			SchemaStorageOverrideRef::new(&self.querier).account_storage_at(at, address, index)
		}

		fn current_block(&self, at: B::Hash) -> Option<ethereum::BlockV3> {
			SchemaStorageOverrideRef::new(&self.querier).current_block(at)
		}

		fn current_receipts(&self, at: B::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
			SchemaStorageOverrideRef::new(&self.querier).current_receipts(at)
		}

		fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
			SchemaStorageOverrideRef::new(&self.querier).current_transaction_statuses(at)
		}

		fn elasticity(&self, at: B::Hash) -> Option<Permill> {
			SchemaStorageOverrideRef::new(&self.querier).elasticity(at)
		}

		fn is_eip1559(&self, at: B::Hash) -> bool {
			SchemaStorageOverrideRef::new(&self.querier).is_eip1559(at)
		}
	}

	/// A storage override for runtimes that use schema v3.
	pub struct SchemaStorageOverrideRef<'a, B, C, BE> {
		querier: &'a StorageQuerier<B, C, BE>,
	}

	impl<'a, B, C, BE> SchemaStorageOverrideRef<'a, B, C, BE> {
		pub fn new(querier: &'a StorageQuerier<B, C, BE>) -> Self {
			Self { querier }
		}
	}

	impl<'a, B, C, BE> StorageOverride<B> for SchemaStorageOverrideRef<'a, B, C, BE>
	where
		B: BlockT,
		C: StorageProvider<B, BE> + Send + Sync,
		BE: Backend<B>,
	{
		fn account_code_at(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
			self.querier.account_code(at, address)
		}

		fn account_storage_at(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
			self.querier.account_storage(at, address, index)
		}

		fn current_block(&self, at: B::Hash) -> Option<ethereum::BlockV3> {
			self.querier.current_block(at)
		}

		fn current_receipts(&self, at: B::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
			self.querier.current_receipts::<ethereum::ReceiptV3>(at)
		}

		fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
			self.querier.current_transaction_statuses(at)
		}

		fn elasticity(&self, at: B::Hash) -> Option<Permill> {
			self.querier.elasticity(at)
		}

		fn is_eip1559(&self, _at: B::Hash) -> bool {
			true
		}
	}
}
