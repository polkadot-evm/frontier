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

use std::{marker::PhantomData, sync::Arc};
use std::collections::BTreeMap;
use ethereum::{Block as EthereumBlock, Transaction as EthereumTransaction};
use ethereum_types::{H160, H256, H64, U256, U64, H512};
use sp_runtime::{
	traits::{Block as BlockT, Header as _, UniqueSaturatedInto, Zero, One, Saturating, BlakeTwo256},
	transaction_validity::TransactionSource
};
use sp_api::{ProvideRuntimeApi, BlockId};
use sc_client_api::backend::{StorageProvider, Backend, StateBackend, AuxStore};
use sha3::{Keccak256, Digest};
use sp_blockchain::{Error as BlockChainError, HeaderMetadata, HeaderBackend, Result};
use sp_storage::StorageKey;
use codec::Decode;
use sp_io::hashing::{twox_128, blake2_128};
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};
use crate::{internal_err, error_on_execution_failure};

pub use fc_rpc_core::{EthApiServer, NetApiServer};
use codec::{self, Encode};

/// Something that can fetch Ethereum-related data from a State Backend with some assumptions
/// about pallet-ethereum's storage schema. This trait is quite similar to the runtime API.
pub trait StorageOverride<Block: BlockT> {

	/// Returns fp_evm::Accounts by address.
	/// TODO Telmo said this requires the address mapping. Is that true? I guess it would be if
	/// we have to query pallet balances directly. But can we query pallet evm for that?
	fn account_basic(&self, block: &BlockId<Block>, address: H160) -> Result<fp_evm::Account>;
	/// Returns FixedGasPrice::min_gas_price
	fn gas_price(&self, block: &BlockId<Block>) -> Result<U256>;
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<Block>, address: H160) -> Result<Vec<u8>>;
	/// Returns the author for the specified block
	fn author(&self, block: &BlockId<Block>) -> Result<H160>;
	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<Block>, address: H160, index: U256) -> Result<H256>;
	/// Return the current block.
	fn current_block(&self, block: &BlockId<Block>) -> Result<Option<EthereumBlock>>;
	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<Block>) -> Result<Option<Vec<ethereum::Receipt>>>;
	/// Return the current transaction status.
	fn current_transaction_statuses(&self, block: &BlockId<Block>) -> Result<Option<Vec<TransactionStatus>>>;
}

pub struct SchemaV1Override<B: BlockT, C, BE> {
	client: Arc<C>,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, C, BE> SchemaV1Override<B, C, BE> {
	pub fn new(
		client: Arc<C>,
	) -> Self {
		Self { client, _marker: PhantomData }
	}
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn blake2_128_extend(bytes: &[u8]) -> Vec<u8> {
	let mut ext: Vec<u8> = blake2_128(bytes).to_vec();
	ext.extend_from_slice(bytes);
	ext
}

impl<B, C, BE> SchemaV1Override<B, C, BE> where
	C: StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
{

	fn current_receipts(&self, id: &BlockId<B>) -> Option<Vec<ethereum::Receipt>> {
		self.query_storage::<Vec<ethereum::Receipt>>(
			id,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentReceipts")
			)
		)
	}

	fn account_storages(&self, id: &BlockId<B>, address: H160, index: U256) -> Option<H256> {
		let tmp: &mut [u8; 32] = &mut [0; 32];
		index.to_little_endian(tmp);

		let mut key: Vec<u8> = storage_prefix_build(b"EVM", b"AccountStorages");
		key.extend(blake2_128_extend(address.as_bytes()));
		key.extend(blake2_128_extend(tmp));

		self.query_storage::<H256>(
			id,
			&StorageKey(key)
		)
	}

	fn query_storage<T: Decode>(&self, id: &BlockId<B>, key: &StorageKey) -> Result<T> {
		// if let Ok(Some(data)) = self.client.storage(
		// 	id,
		// 	key
		// ) {
		// 	if let Ok(result) = Decode::decode(&mut &data.0[..]) {
		// 		return Some(result);
		// 	}
		// }
		// None

		let raw_data = self.client.storage(id, key)?
			.ok_or("Storage provider returned no data".into())?;

		Decode::decode(&mut &raw_data.0[..]).map_err("Could not decode data".into())
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
	fn account_basic(&self, block: &BlockId<Block>, address: H160) -> Result<fp_evm::Account> {
		unimplemented!()
	}
	/// Returns FixedGasPrice::min_gas_price
	fn gas_price(&self, block: &BlockId<Block>) -> Result<U256> {
		unimplemented!()
	}
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, id: &BlockId<Block>, address: H160) -> Result<Vec<u8>> {
		let mut key: Vec<u8> = storage_prefix_build(b"EVM", b"AccountCodes");
		key.extend(blake2_128_extend(address.as_bytes()));
		self.query_storage::<Vec<u8>>(
			id,
			&StorageKey(key)
		)
	}
	/// Returns the author for the specified block
	fn author(&self, block: &BlockId<Block>) -> Result<H160> {
		unimplemented!()
	}
	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<Block>, address: H160, index: U256) -> Result<H256> {
		unimplemented!()
	}
	/// Return the current block.
	fn current_block(&self, block: &BlockId<Block>) -> Result<Option<EthereumBlock>> {
		self.query_storage::<ethereum::Block>(
			block,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentBlock")
			)
		)
	}
	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<Block>) -> Result<Option<Vec<ethereum::Receipt>>> {
		unimplemented!()
	}
	/// Return the current transaction status.
	fn current_transaction_statuses(&self, block: &BlockId<Block>) -> Result<Option<Vec<TransactionStatus>>> {
		self.query_storage::<Vec<TransactionStatus>>(
			block,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentTransactionStatuses")
			)
		)
	}
}
