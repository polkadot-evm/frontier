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
use jsonrpc_core::{BoxFuture, Result, futures::future::{self, Future}};
use sp_runtime::{
	traits::{Block as BlockT, Header as _, UniqueSaturatedInto, Zero, One, Saturating, BlakeTwo256},
	transaction_validity::TransactionSource
};
use sp_api::{ProvideRuntimeApi, BlockId};
use sc_client_api::backend::{StorageProvider, Backend, StateBackend, AuxStore};
use sha3::{Keccak256, Digest};
use sp_blockchain::{Error as BlockChainError, HeaderMetadata, HeaderBackend};
use sp_storage::StorageKey;
use codec::Decode;
use sp_io::hashing::{twox_128, blake2_128};
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};
use crate::{internal_err, error_on_execution_failure, eth::StorageOverride};

pub use fc_rpc_core::{EthApiServer, NetApiServer};
use codec::{self, Encode};

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
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	C: Send + Sync + 'static,
{

	fn headers(&self, id: &BlockId<B>) -> Result<(u64,u64)> {
		match self.client.header(id.clone())
			.map_err(|_| internal_err(format!("failed to retrieve header at: {:#?}", id)))?
		{
			Some(h) => {
				let best_number: u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(
					self.client.info().best_number
				);
				let header_number: u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(
					*h.number()
				);
				Ok((best_number, header_number))
			}
			_ => Err(internal_err(format!("failed to retrieve header at: {:#?}", id)))
		}
	}

	fn current_block(&self, id: &BlockId<B>) -> Option<ethereum::Block> {
		self.query_storage::<ethereum::Block>(
			id,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentBlock")
			)
		)
	}

	fn current_statuses(&self, id: &BlockId<B>) -> Option<Vec<TransactionStatus>> {
		self.query_storage::<Vec<TransactionStatus>>(
			id,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentTransactionStatuses")
			)
		)
	}

	fn current_receipts(&self, id: &BlockId<B>) -> Option<Vec<ethereum::Receipt>> {
		self.query_storage::<Vec<ethereum::Receipt>>(
			id,
			&StorageKey(
				storage_prefix_build(b"Ethereum", b"CurrentReceipts")
			)
		)
	}

	fn account_codes(&self, id: &BlockId<B>, address: H160) -> Option<Vec<u8>> {
		let mut key: Vec<u8> = storage_prefix_build(b"EVM", b"AccountCodes");
		key.extend(blake2_128_extend(address.as_bytes()));
		self.query_storage::<Vec<u8>>(
			id,
			&StorageKey(key)
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

impl<Block: BlockT, C, BE> StorageOverride<Block> for SchemaV1Override<Block, C, BE> {
	fn account_basic(&self, block: &BlockId<Block>, address: H160) -> Result<fp_evm::Account> {
		unimplemented!()
	}
	/// Returns FixedGasPrice::min_gas_price
	fn gas_price(&self, block: &BlockId<Block>) -> Result<U256> {
		unimplemented!()
	}
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<Block>, address: H160) -> Result<Vec<u8>> {
		unimplemented!()
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
		unimplemented!()
	}
	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<Block>) -> Result<Option<Vec<ethereum::Receipt>>> {
		unimplemented!()
	}
	/// Return the current transaction status.
	fn current_transaction_statuses(&self, block: &BlockId<Block>) -> Result<Option<Vec<TransactionStatus>>> {
		unimplemented!()
	}
}
