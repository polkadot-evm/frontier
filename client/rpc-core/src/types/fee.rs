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

use std::{
	collections::BTreeMap,
	sync::{Arc, Mutex},
};

use ethereum_types::U256;
use serde::Serialize;

/// `eth_feeHistory` response
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeHistory {
	/// Lowest number block of the returned range.
	pub oldest_block: U256,
	/// An array of block base fees per gas.
	/// This includes the next block after the newest of the returned range,
	/// because this value can be derived from the newest block. Zeroes are
	/// returned for pre-EIP-1559 blocks.
	pub base_fee_per_gas: Vec<U256>,
	/// An array of block gas used ratios. These are calculated as the ratio
	/// of gasUsed and gasLimit.
	pub gas_used_ratio: Vec<f64>,
	/// An array of effective priority fee per gas data points from a single
	/// block. All zeroes are returned if the block is empty.
	pub reward: Option<Vec<Vec<U256>>>,
}

pub type FeeHistoryCache = Arc<Mutex<BTreeMap<u64, FeeHistoryCacheItem>>>;
/// Maximum fee history cache size.
pub type FeeHistoryCacheLimit = u64;

pub struct FeeHistoryCacheItem {
	pub base_fee: u64,
	pub gas_used_ratio: f64,
	pub rewards: Vec<u64>,
}
