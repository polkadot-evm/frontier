// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
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

/// Current version of pallet Ethereum's storage schema is stored under this key.
pub const PALLET_ETHEREUM_SCHEMA: &'static [u8] = b":ethereum_schema";
/// Cached version of pallet Ethereum's storage schema is stored under this key in the AuxStore.
pub const PALLET_ETHEREUM_SCHEMA_CACHE: &'static [u8] = b":ethereum_schema_cache";

/// The schema version for Pallet Ethereum's storage
#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord)]
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
