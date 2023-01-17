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
use scale_codec::Decode;
// Substrate
use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sp_api::BlockId;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	traits::{BlakeTwo256, Block as BlockT, Header as HeaderT},
	Permill,
};
use sp_storage::StorageKey;
// Frontier
use fp_rpc::TransactionStatus;
use fp_storage::*;

use super::{blake2_128_extend, storage_prefix_build, StorageOverride};

/// An override for runtimes that use Schema V3
pub struct SchemaV3Override<B: BlockT, C, BE> {
	client: Arc<C>,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, C, BE> SchemaV3Override<B, C, BE> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<B, C, BE> SchemaV3Override<B, C, BE>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: StorageProvider<B, BE> + HeaderBackend<B> + Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	fn query_storage<T: Decode>(&self, id: &BlockId<B>, key: &StorageKey) -> Option<T> {
		if let Ok(Some(header)) = self.client.header(*id) {
			if let Ok(Some(data)) = self.client.storage(header.hash(), key) {
				if let Ok(result) = Decode::decode(&mut &data.0[..]) {
					return Some(result);
				}
			}
		}
		None
	}
}

impl<B, C, BE> StorageOverride<B> for SchemaV3Override<B, C, BE>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: StorageProvider<B, BE> + HeaderBackend<B> + Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<B>, address: H160) -> Option<Vec<u8>> {
		let mut key: Vec<u8> = storage_prefix_build(PALLET_EVM, EVM_ACCOUNT_CODES);
		key.extend(blake2_128_extend(address.as_bytes()));
		self.query_storage::<Vec<u8>>(block, &StorageKey(key))
	}

	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<B>, address: H160, index: U256) -> Option<H256> {
		let tmp: &mut [u8; 32] = &mut [0; 32];
		index.to_big_endian(tmp);

		let mut key: Vec<u8> = storage_prefix_build(PALLET_EVM, EVM_ACCOUNT_STORAGES);
		key.extend(blake2_128_extend(address.as_bytes()));
		key.extend(blake2_128_extend(tmp));

		self.query_storage::<H256>(block, &StorageKey(key))
	}

	/// Return the current block.
	fn current_block(&self, block: &BlockId<B>) -> Option<ethereum::BlockV2> {
		self.query_storage::<ethereum::BlockV2>(
			block,
			&StorageKey(storage_prefix_build(
				PALLET_ETHEREUM,
				ETHEREUM_CURRENT_BLOCK,
			)),
		)
	}

	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<B>) -> Option<Vec<ethereum::ReceiptV3>> {
		self.query_storage::<Vec<ethereum::ReceiptV3>>(
			block,
			&StorageKey(storage_prefix_build(
				PALLET_ETHEREUM,
				ETHEREUM_CURRENT_RECEIPTS,
			)),
		)
	}

	/// Return the current transaction status.
	fn current_transaction_statuses(&self, block: &BlockId<B>) -> Option<Vec<TransactionStatus>> {
		self.query_storage::<Vec<TransactionStatus>>(
			block,
			&StorageKey(storage_prefix_build(
				PALLET_ETHEREUM,
				ETHEREUM_CURRENT_TRANSACTION_STATUS,
			)),
		)
	}

	/// Return the elasticity at the given height.
	fn elasticity(&self, block: &BlockId<B>) -> Option<Permill> {
		let default_elasticity = Some(Permill::from_parts(125_000));
		let elasticity = self.query_storage::<Permill>(
			block,
			&StorageKey(storage_prefix_build(PALLET_BASE_FEE, BASE_FEE_ELASTICITY)),
		);
		if elasticity.is_some() {
			elasticity
		} else {
			default_elasticity
		}
	}

	fn is_eip1559(&self, _block: &BlockId<B>) -> bool {
		true
	}
}
