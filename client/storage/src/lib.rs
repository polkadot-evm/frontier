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

#![warn(unused_crate_dependencies)]

pub mod overrides;

use std::sync::Arc;

use ethereum::{BlockV2, ReceiptV3};
use ethereum_types::{Address, H256, U256};
// Substrate
use sc_client_api::{backend::Backend, StorageProvider};
use sp_api::ProvideRuntimeApi;
use sp_runtime::{traits::Block as BlockT, Permill};
// Frontier
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};
use fp_storage::EthereumStorageSchema;

pub use self::overrides::*;

/// A storage override for runtimes that use different ethereum schema.
///
/// It fetches data from the state backend, with some assumptions about pallet-ethereum's storage
/// schema, as a preference. However, if there is no ethereum schema in the state, it'll use the
/// runtime API as fallback implementation.
///
/// It is used to avoid spawning the runtime and the overhead associated with it.
#[derive(Clone)]
pub struct StorageOverrideHandler<B, C, BE> {
	querier: StorageQuerier<B, C, BE>,
	fallback: RuntimeApiStorageOverride<B, C>,
}

impl<B, C, BE> StorageOverrideHandler<B, C, BE> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			querier: StorageQuerier::new(client.clone()),
			fallback: RuntimeApiStorageOverride::<B, C>::new(client),
		}
	}
}

impl<B, C, BE> StorageOverride<B> for StorageOverrideHandler<B, C, BE>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: StorageProvider<B, BE> + Send + Sync + 'static,
	BE: Backend<B> + 'static,
{
	fn account_code_at(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
		match self.querier.storage_schema(at) {
			Some(EthereumStorageSchema::V1) => {
				SchemaV1StorageOverrideRef::new(&self.querier).account_code_at(at, address)
			}
			Some(EthereumStorageSchema::V2) => {
				SchemaV2StorageOverrideRef::new(&self.querier).account_code_at(at, address)
			}
			Some(EthereumStorageSchema::V3) => {
				SchemaV3StorageOverrideRef::new(&self.querier).account_code_at(at, address)
			}
			None => self.fallback.account_code_at(at, address),
		}
	}

	fn account_storage_at(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
		match self.querier.storage_schema(at) {
			Some(EthereumStorageSchema::V1) => SchemaV1StorageOverrideRef::new(&self.querier)
				.account_storage_at(at, address, index),
			Some(EthereumStorageSchema::V2) => SchemaV2StorageOverrideRef::new(&self.querier)
				.account_storage_at(at, address, index),
			Some(EthereumStorageSchema::V3) => SchemaV3StorageOverrideRef::new(&self.querier)
				.account_storage_at(at, address, index),
			None => self.fallback.account_storage_at(at, address, index),
		}
	}

	fn current_block(&self, at: B::Hash) -> Option<BlockV2> {
		match self.querier.storage_schema(at) {
			Some(EthereumStorageSchema::V1) => {
				SchemaV1StorageOverrideRef::new(&self.querier).current_block(at)
			}
			Some(EthereumStorageSchema::V2) => {
				SchemaV2StorageOverrideRef::new(&self.querier).current_block(at)
			}
			Some(EthereumStorageSchema::V3) => {
				SchemaV3StorageOverrideRef::new(&self.querier).current_block(at)
			}
			None => self.fallback.current_block(at),
		}
	}

	fn current_receipts(&self, at: B::Hash) -> Option<Vec<ReceiptV3>> {
		match self.querier.storage_schema(at) {
			Some(EthereumStorageSchema::V1) => {
				SchemaV1StorageOverrideRef::new(&self.querier).current_receipts(at)
			}
			Some(EthereumStorageSchema::V2) => {
				SchemaV2StorageOverrideRef::new(&self.querier).current_receipts(at)
			}
			Some(EthereumStorageSchema::V3) => {
				SchemaV3StorageOverrideRef::new(&self.querier).current_receipts(at)
			}
			None => self.fallback.current_receipts(at),
		}
	}

	fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
		match self.querier.storage_schema(at) {
			Some(EthereumStorageSchema::V1) => {
				SchemaV1StorageOverrideRef::new(&self.querier).current_transaction_statuses(at)
			}
			Some(EthereumStorageSchema::V2) => {
				SchemaV2StorageOverrideRef::new(&self.querier).current_transaction_statuses(at)
			}
			Some(EthereumStorageSchema::V3) => {
				SchemaV3StorageOverrideRef::new(&self.querier).current_transaction_statuses(at)
			}
			None => self.fallback.current_transaction_statuses(at),
		}
	}

	fn elasticity(&self, at: B::Hash) -> Option<Permill> {
		match self.querier.storage_schema(at) {
			Some(EthereumStorageSchema::V1) => {
				SchemaV1StorageOverrideRef::new(&self.querier).elasticity(at)
			}
			Some(EthereumStorageSchema::V2) => {
				SchemaV2StorageOverrideRef::new(&self.querier).elasticity(at)
			}
			Some(EthereumStorageSchema::V3) => {
				SchemaV3StorageOverrideRef::new(&self.querier).elasticity(at)
			}
			None => self.fallback.elasticity(at),
		}
	}

	fn is_eip1559(&self, at: B::Hash) -> bool {
		match self.querier.storage_schema(at) {
			Some(EthereumStorageSchema::V1) => {
				SchemaV1StorageOverrideRef::new(&self.querier).is_eip1559(at)
			}
			Some(EthereumStorageSchema::V2) => {
				SchemaV2StorageOverrideRef::new(&self.querier).is_eip1559(at)
			}
			Some(EthereumStorageSchema::V3) => {
				SchemaV3StorageOverrideRef::new(&self.querier).is_eip1559(at)
			}
			None => self.fallback.is_eip1559(at),
		}
	}
}
