// This file is part of Tokfin.

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

use ethereum_types::{Address, H256, U256};
use serde::{Deserialize, Serialize};

/// A list of addresses and storage keys.
///
/// These addresses and storage keys are added into the `accessed_addresses` and
/// `accessed_storage_keys` global sets (introduced in [EIP-2929](https://eips.ethereum.org/EIPS/eip-2929)).
///
/// A gas cost is charged, though at a discount relative to the cost of accessing outside the list.
pub type AccessList = Vec<AccessListItem>;

/// The item of access list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListItem {
	pub address: Address,
	pub storage_keys: Vec<H256>,
}

/// The response type of `eth_createAccessList`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListResult {
	pub access_list: AccessList,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
	pub gas_used: U256,
}
