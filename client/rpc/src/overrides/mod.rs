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
use sp_runtime::traits::Block as BlockT;
use sp_api::BlockId;
use sp_io::hashing::{twox_128, blake2_128};
use fp_rpc::TransactionStatus;

mod schema_v1_override;

pub use fc_rpc_core::{EthApiServer, NetApiServer};
pub use schema_v1_override::SchemaV1Override;

/// Something that can fetch Ethereum-related data from a State Backend with some assumptions
/// about pallet-ethereum's storage schema. This trait is quite similar to the runtime API.
pub trait StorageOverride<Block: BlockT> {
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<Block>, address: H160) -> Option<Vec<u8>>;
	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<Block>, address: H160, index: U256) -> Option<H256>;
	/// Return the current block.
	fn current_block(&self, block: &BlockId<Block>) -> Option<EthereumBlock>;
	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<Block>) -> Option<Vec<ethereum::Receipt>>;
	/// Return the current transaction status.
	fn current_transaction_statuses(&self, block: &BlockId<Block>) -> Option<Vec<TransactionStatus>>;
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn blake2_128_extend(bytes: &[u8]) -> Vec<u8> {
	let mut ext: Vec<u8> = blake2_128(bytes).to_vec();
	ext.extend_from_slice(bytes);
	ext
}
