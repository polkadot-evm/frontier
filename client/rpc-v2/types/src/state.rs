// This file is part of Frontier.

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

use std::collections::HashMap;

use ethereum_types::{Address, H256, U256, U64};
use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;

pub type StateOverrides = HashMap<Address, AccountOverride>;

/// Indicates the overriding fields of account during the execution of a message call.
///
/// Note, state and stateDiff can't be specified at the same time.
/// If state is set, message execution will only use the data in the given state.
/// Otherwise, if statDiff is set, all diff will be applied first and then execute the call message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AccountOverride {
	/// Fake balance to set for the account before executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub balance: Option<U256>,
	/// Fake nonce to set for the account before executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub nonce: Option<U64>,
	/// Fake EVM bytecode to inject into the account before executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub code: Option<Bytes>,
	/// Fake key-value mapping to override all slots in the account storage before
	/// executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub state: Option<HashMap<H256, H256>>,
	/// Fake key-value mapping to override individual slots in the account storage before
	/// executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub state_diff: Option<HashMap<H256, H256>>,
}
