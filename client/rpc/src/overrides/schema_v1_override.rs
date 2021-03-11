// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Frontier.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use ethereum::Block as EthereumBlock;
use ethereum_types::{H160, H256, U256};
use std::{marker::PhantomData, sync::Arc};
use sc_client_api::backend::{StorageProvider, Backend, StateBackend, AuxStore};
use sp_blockchain::{Error as BlockChainError, HeaderMetadata, HeaderBackend};
use sp_storage::StorageKey;
use codec::Decode;
use sp_runtime::traits::{Block as BlockT, BlakeTwo256};
use sp_api::BlockId;
use fp_rpc::TransactionStatus;

use super::{StorageOverride, storage_prefix_build, blake2_128_extend};

/// An override for runtimes that use Schema V1
pub struct SchemaV1Override<B: BlockT, C, BE> {
	client: Arc<C>,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, C, BE> SchemaV1Override<B, C, BE> {
	pub fn new(client: Arc<C>) -> Self {
		Self { client, _marker: PhantomData }
	}
}

impl<B, C, BE> SchemaV1Override<B, C, BE> where
	C: StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
{
	// My attempt using result
	// fn query_storage<T: Decode>(&self, id: &BlockId<B>, key: &StorageKey) -> Result<T> {
	// 	let raw_data = self.client.storage(id, key)?
	// 		.ok_or("Storage provider returned Ok(None)")?;
	//
	// 	Decode::decode(&mut &raw_data.0[..]).map_err(|_| "Could not decode data".into())
	// }

	fn query_storage<T: Decode>(&self, id: &BlockId<B>, key: &StorageKey) -> Option<T> {
		if let Ok(Some(data)) = self.client.storage(
			id,
			key
		) {
			if let Ok(result) = Decode::decode(&mut &data.0[..]) {
				return Some(result);
			}
		}
		None
	}
}

impl<Block, C, BE> StorageOverride<Block> for SchemaV1Override<Block, C, BE>
where
	C: StorageProvider<Block, BE>,
	C: AuxStore,
	C: HeaderBackend<Block>,
	C: HeaderMetadata<Block, Error=BlockChainError> + 'static,
	BE: Backend<Block> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	Block: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
{
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<Block>, address: H160) -> Option<Vec<u8>> {
		let mut key: Vec<u8> = storage_prefix_build(b"EVM", b"AccountCodes");
		key.extend(blake2_128_extend(address.as_bytes()));
		self.query_storage::<Vec<u8>>(
			block,
			&StorageKey(key)
		)
	}

	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<Block>, address: H160, index: U256) -> Option<H256> {
		let tmp: &mut [u8; 32] = &mut [0; 32];
		index.to_little_endian(tmp);

		let mut key: Vec<u8> = storage_prefix_build(b"EVM", b"AccountStorages");
		key.extend(blake2_128_extend(address.as_bytes()));
		key.extend(blake2_128_extend(tmp));

		self.query_storage::<H256>(
			block,
			&StorageKey(key)
		)
	}

	/// Return the current block.
	fn current_block(&self, block: &BlockId<Block>) -> Option<EthereumBlock> {
		self.query_storage::<ethereum::Block>(
			block,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentBlock")
			)
		)
	}

	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<Block>) -> Option<Vec<ethereum::Receipt>> {
		self.query_storage::<Vec<ethereum::Receipt>>(
			block,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentReceipts")
			)
		)
	}

	/// Return the current transaction status.
	fn current_transaction_statuses(&self, block: &BlockId<Block>) -> Option<Vec<TransactionStatus>> {
		self.query_storage::<Vec<TransactionStatus>>(
			block,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentTransactionStatuses")
			)
		)
	}
}
