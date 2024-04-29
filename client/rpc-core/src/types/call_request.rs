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

use std::collections::BTreeMap;

use ethereum_types::{H256, U256};
use serde::Deserialize;

use crate::types::Bytes;

// State override
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CallStateOverride {
	/// Fake balance to set for the account before executing the call.
	pub balance: Option<U256>,
	/// Fake nonce to set for the account before executing the call.
	pub nonce: Option<U256>,
	/// Fake EVM bytecode to inject into the account before executing the call.
	pub code: Option<Bytes>,
	/// Fake key-value mapping to override all slots in the account storage before
	/// executing the call.
	pub state: Option<BTreeMap<H256, H256>>,
	/// Fake key-value mapping to override individual slots in the account storage before
	/// executing the call.
	pub state_diff: Option<BTreeMap<H256, H256>>,
}
