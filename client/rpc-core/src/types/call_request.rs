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
