// Copyright (C) 2022 Deeper Network Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{marker::PhantomData, sync::Arc};

use codec::Decode;
use ethereum_types::{H160, H256, U256};
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
use fp_rpc::TransactionStatusV2;

use super::{blake2_128_extend, storage_prefix_build, StorageOverride};
/// An override for runtimes that use Schema V1
pub struct SchemaV4Override<B: BlockT, C, BE> {
	client: Arc<C>,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, C, BE> SchemaV4Override<B, C, BE> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<B, C, BE> SchemaV4Override<B, C, BE>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: StorageProvider<B, BE> + HeaderBackend<B> + Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	fn query_storage<T: Decode>(&self, id: &BlockId<B>, key: &StorageKey) -> Option<T> {
		if let Ok(Some(header)) = self.client.header(*id) {
			if let Ok(Some(data)) = self.client.storage(&header.hash(), key) {
				if let Ok(result) = Decode::decode(&mut &data.0[..]) {
					return Some(result);
				}
			}
		}
		None
	}
}

impl<B, C, BE> StorageOverride<B> for SchemaV4Override<B, C, BE>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: StorageProvider<B, BE> + HeaderBackend<B> + Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<B>, address: H160) -> Option<Vec<u8>> {
		let mut key: Vec<u8> = storage_prefix_build(b"EVM", b"AccountCodes");
		key.extend(blake2_128_extend(address.as_bytes()));
		self.query_storage::<Vec<u8>>(block, &StorageKey(key))
	}

	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<B>, address: H160, index: U256) -> Option<H256> {
		let tmp: &mut [u8; 32] = &mut [0; 32];
		index.to_big_endian(tmp);

		let mut key: Vec<u8> = storage_prefix_build(b"EVM", b"AccountStorages");
		key.extend(blake2_128_extend(address.as_bytes()));
		key.extend(blake2_128_extend(tmp));

		self.query_storage::<H256>(block, &StorageKey(key))
	}

	/// Return the current block.
	fn current_block(&self, block: &BlockId<B>) -> Option<ethereum::BlockV2> {
		self.query_storage::<ethereum::BlockV2>(
			block,
			&StorageKey(storage_prefix_build(b"Ethereum", b"CurrentBlock")),
		)
	}

	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<B>) -> Option<Vec<ethereum::ReceiptV3>> {
		self.query_storage::<Vec<ethereum::ReceiptV3>>(
			block,
			&StorageKey(storage_prefix_build(b"Ethereum", b"CurrentReceipts")),
		)
	}

	/// Return the current transaction status.
	fn current_transaction_statuses(&self, block: &BlockId<B>) -> Option<Vec<TransactionStatusV2>> {
		self.query_storage::<Vec<TransactionStatusV2>>(
			block,
			&StorageKey(storage_prefix_build(
				b"Ethereum",
				b"CurrentTransactionStatuses",
			)),
		)
	}

	/// Prior to eip-1559 there is no base fee.
	fn elasticity(&self, block: &BlockId<B>) -> Option<Permill> {
		let default_elasticity = Some(Permill::from_parts(125_000));
		let elasticity = self.query_storage::<Permill>(
			block,
			&StorageKey(storage_prefix_build(b"BaseFee", b"Elasticity")),
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
