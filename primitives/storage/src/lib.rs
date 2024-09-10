// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

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
#![warn(unused_crate_dependencies)]

use scale_codec::{Decode, Encode};

/// Some storage constants
pub mod constants {
	/// Pallet Evm storage items
	pub const PALLET_EVM: &[u8] = b"EVM";
	pub const EVM_ACCOUNT_CODES: &[u8] = b"AccountCodes";
	pub const EVM_ACCOUNT_STORAGES: &[u8] = b"AccountStorages";

	/// Pallet Ethereum storage items
	pub const PALLET_ETHEREUM: &[u8] = b"Ethereum";
	pub const ETHEREUM_CURRENT_BLOCK: &[u8] = b"CurrentBlock";
	pub const ETHEREUM_CURRENT_RECEIPTS: &[u8] = b"CurrentReceipts";
	pub const ETHEREUM_CURRENT_TRANSACTION_STATUSES: &[u8] = b"CurrentTransactionStatuses";

	/// Pallet BaseFee storage items
	pub const PALLET_BASE_FEE: &[u8] = b"BaseFee";
	pub const BASE_FEE_PER_GAS: &[u8] = b"BaseFeePerGas";
	pub const BASE_FEE_ELASTICITY: &[u8] = b"Elasticity";
}

/// Current version of pallet Ethereum's storage schema is stored under this key.
pub const PALLET_ETHEREUM_SCHEMA: &[u8] = b":ethereum_schema";
/// Cached version of pallet Ethereum's storage schema is stored under this key in the AuxStore.
pub const PALLET_ETHEREUM_SCHEMA_CACHE: &[u8] = b":ethereum_schema_cache";

/// The schema version for Pallet Ethereum's storage
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EthereumStorageSchema {
	// deprecated
	// #[codec(index = 0)]
	// Undefined,
	#[codec(index = 1)]
	V1,
	#[codec(index = 2)]
	V2,
	#[codec(index = 3)]
	V3,
}
