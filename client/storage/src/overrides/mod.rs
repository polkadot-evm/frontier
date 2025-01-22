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
use scale_codec::Decode;
// Substrate
use sc_client_api::{Backend, StorageProvider};
use sp_io::hashing::{blake2_128, twox_128};
use sp_runtime::{traits::Block as BlockT, Permill};
use sp_storage::StorageKey;
// Frontier
use fp_rpc::TransactionStatus;
use fp_storage::{constants::*, EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};

mod runtime_api;
mod schema;

pub use self::{
	runtime_api::RuntimeApiStorageOverride,
	schema::{
		v1::{
			SchemaStorageOverride as SchemaV1StorageOverride,
			SchemaStorageOverrideRef as SchemaV1StorageOverrideRef,
		},
		v2::{
			SchemaStorageOverride as SchemaV2StorageOverride,
			SchemaStorageOverrideRef as SchemaV2StorageOverrideRef,
		},
		v3::{
			SchemaStorageOverride as SchemaV3StorageOverride,
			SchemaStorageOverrideRef as SchemaV3StorageOverrideRef,
		},
	},
};

/// This trait is used to obtain Ethereum-related data.
pub trait StorageOverride<Block: BlockT>: Send + Sync {
	/// Return the code with the given address.
	fn account_code_at(&self, at: Block::Hash, address: Address) -> Option<Vec<u8>>;
	/// Return the storage data with the given address and storage index.
	fn account_storage_at(&self, at: Block::Hash, address: Address, index: U256) -> Option<H256>;

	/// Return the current ethereum block.
	fn current_block(&self, at: Block::Hash) -> Option<ethereum::BlockV2>;
	/// Return the current ethereum transaction receipt.
	fn current_receipts(&self, at: Block::Hash) -> Option<Vec<ethereum::ReceiptV3>>;
	/// Return the current ethereum transaction status.
	fn current_transaction_statuses(&self, at: Block::Hash) -> Option<Vec<TransactionStatus>>;

	/// Return the elasticity multiplier at the given post-eip1559 block.
	fn elasticity(&self, at: Block::Hash) -> Option<Permill>;
	/// Return `true` if the request block is post-eip1559.
	fn is_eip1559(&self, at: Block::Hash) -> bool;
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn blake2_128_extend(bytes: &[u8]) -> Vec<u8> {
	let mut ext: Vec<u8> = blake2_128(bytes).to_vec();
	ext.extend_from_slice(bytes);
	ext
}

/// A useful utility for querying storage.
#[derive(Clone)]
pub struct StorageQuerier<B, C, BE> {
	client: Arc<C>,
	_marker: PhantomData<(B, BE)>,
}

impl<B, C, BE> StorageQuerier<B, C, BE> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<B, C, BE> StorageQuerier<B, C, BE>
where
	B: BlockT,
	C: StorageProvider<B, BE>,
	BE: Backend<B>,
{
	pub fn query<T: Decode>(&self, at: B::Hash, key: &StorageKey) -> Option<T> {
		if let Ok(Some(data)) = self.client.storage(at, key) {
			if let Ok(result) = Decode::decode(&mut &data.0[..]) {
				return Some(result);
			}
		}
		None
	}

	pub fn storage_schema(&self, at: B::Hash) -> Option<EthereumStorageSchema> {
		let key = PALLET_ETHEREUM_SCHEMA.to_vec();
		self.query::<EthereumStorageSchema>(at, &StorageKey(key))
	}

	pub fn account_code(&self, at: B::Hash, address: Address) -> Option<Vec<u8>> {
		let mut key: Vec<u8> = storage_prefix_build(PALLET_EVM, EVM_ACCOUNT_CODES);
		key.extend(blake2_128_extend(address.as_bytes()));
		self.query::<Vec<u8>>(at, &StorageKey(key))
	}

	pub fn account_storage(&self, at: B::Hash, address: Address, index: U256) -> Option<H256> {
		let tmp: &mut [u8; 32] = &mut [0; 32];
		index.write_as_big_endian(tmp);

		let mut key: Vec<u8> = storage_prefix_build(PALLET_EVM, EVM_ACCOUNT_STORAGES);
		key.extend(blake2_128_extend(address.as_bytes()));
		key.extend(blake2_128_extend(tmp));

		self.query::<H256>(at, &StorageKey(key))
	}

	pub fn current_block<Block: Decode>(&self, at: B::Hash) -> Option<Block> {
		let key = storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_BLOCK);
		self.query::<Block>(at, &StorageKey(key))
	}

	pub fn current_receipts<Receipt: Decode>(&self, at: B::Hash) -> Option<Vec<Receipt>> {
		let key = storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_RECEIPTS);
		self.query::<Vec<Receipt>>(at, &StorageKey(key))
	}

	pub fn current_transaction_statuses(&self, at: B::Hash) -> Option<Vec<TransactionStatus>> {
		let key = storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_TRANSACTION_STATUSES);
		self.query::<Vec<TransactionStatus>>(at, &StorageKey(key))
	}

	pub fn elasticity(&self, at: B::Hash) -> Option<Permill> {
		let key = storage_prefix_build(PALLET_BASE_FEE, BASE_FEE_ELASTICITY);
		self.query::<Permill>(at, &StorageKey(key))
	}
}
