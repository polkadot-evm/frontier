// Copyright (C) 2022 Deeper Network Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use ethereum_types::{H160, H256, U256};
use evm::ExitReason;
use serde::Serialize;

/// Status
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
	/// Transaction Hash
	pub transaction_hash: Option<H256>,
	/// Transaction index
	pub transaction_index: Option<U256>,
	/// Sender
	pub from: Option<H160>,
	/// Recipient
	pub to: Option<H160>,
	/// Contract address
	pub contract_address: Option<H160>,
	/// Reason
	pub reason: Option<ExitReason>,
}
