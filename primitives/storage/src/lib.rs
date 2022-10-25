// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};

use ethereum_types::{H160, H256, U256};
use sp_api::BlockId;
use sp_runtime::{traits::Block as BlockT, Permill};

use sp_std::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};

/// Current version of pallet Ethereum's storage schema is stored under this key.
pub const PALLET_ETHEREUM_SCHEMA: &[u8] = b":ethereum_schema";
/// Cached version of pallet Ethereum's storage schema is stored under this key in the AuxStore.
pub const PALLET_ETHEREUM_SCHEMA_CACHE: &[u8] = b":ethereum_schema_cache";

/// Pallet Evm storage items
pub const PALLET_EVM: &[u8] = b"EVM";
pub const EVM_ACCOUNT_CODES: &[u8] = b"AccountCodes";
pub const EVM_ACCOUNT_STORAGES: &[u8] = b"AccountStorages";

/// Pallet Ethereum storage items
pub const PALLET_ETHEREUM: &[u8] = b"Ethereum";
pub const ETHEREUM_CURRENT_BLOCK: &[u8] = b"CurrentBlock";
pub const ETHEREUM_CURRENT_RECEIPTS: &[u8] = b"CurrentReceipts";
pub const ETHEREUM_CURRENT_TRANSACTION_STATUS: &[u8] = b"CurrentTransactionStatuses";

/// Pallet BaseFee storage items
pub const PALLET_BASE_FEE: &[u8] = b"BaseFee";
pub const BASE_FEE_PER_GAS: &[u8] = b"BaseFeePerGas";
pub const BASE_FEE_ELASTICITY: &[u8] = b"Elasticity";

/// The schema version for Pallet Ethereum's storage
#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum EthereumStorageSchema {
	Undefined,
	V1,
	V2,
	V3,
}

impl Default for EthereumStorageSchema {
	fn default() -> Self {
		Self::Undefined
	}
}

pub struct OverrideHandle<Block: BlockT> {
	pub schemas: BTreeMap<EthereumStorageSchema, Box<dyn StorageOverride<Block> + Send + Sync>>,
	pub fallback: Box<dyn StorageOverride<Block> + Send + Sync>,
}

/// Something that can fetch Ethereum-related data. This trait is quite similar to the runtime API,
/// and indeed oe implementation of it uses the runtime API.
/// Having this trait is useful because it allows optimized implementations that fetch data from a
/// State Backend with some assumptions about pallet-ethereum's storage schema. Using such an
/// optimized implementation avoids spawning a runtime and the overhead associated with it.
pub trait StorageOverride<Block: BlockT> {
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block: &BlockId<Block>, address: H160) -> Option<Vec<u8>>;
	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block: &BlockId<Block>, address: H160, index: U256) -> Option<H256>;
	/// Return the current block.
	fn current_block(&self, block: &BlockId<Block>) -> Option<ethereum::BlockV2>;
	/// Return the current receipt.
	fn current_receipts(&self, block: &BlockId<Block>) -> Option<Vec<ethereum::ReceiptV3>>;
	/// Return the current transaction status.
	fn current_transaction_statuses(
		&self,
		block: &BlockId<Block>,
	) -> Option<Vec<fp_rpc::TransactionStatus>>;
	/// Return the base fee at the given height.
	fn base_fee(&self, block: &BlockId<Block>) -> Option<U256>;
	/// Return the base fee at the given height.
	fn elasticity(&self, block: &BlockId<Block>) -> Option<Permill>;
	/// Return `true` if the request BlockId is post-eip1559.
	fn is_eip1559(&self, block: &BlockId<Block>) -> bool;
}
