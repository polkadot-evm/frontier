// This file is part of Frontier.

// Copyright (c) Moonsong Labs.
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

use crate::EvmResult;
use alloc::{vec, vec::Vec};
use pallet_evm::{Log, PrecompileHandle};
use sp_core::{H160, H256};

/// Create a 0-topic log.
#[must_use]
pub fn log0(address: impl Into<H160>, data: impl Into<Vec<u8>>) -> Log {
	Log {
		address: address.into(),
		topics: vec![],
		data: data.into(),
	}
}

/// Create a 1-topic log.
#[must_use]
pub fn log1(address: impl Into<H160>, topic0: impl Into<H256>, data: impl Into<Vec<u8>>) -> Log {
	Log {
		address: address.into(),
		topics: vec![topic0.into()],
		data: data.into(),
	}
}

/// Create a 2-topics log.
#[must_use]
pub fn log2(
	address: impl Into<H160>,
	topic0: impl Into<H256>,
	topic1: impl Into<H256>,
	data: impl Into<Vec<u8>>,
) -> Log {
	Log {
		address: address.into(),
		topics: vec![topic0.into(), topic1.into()],
		data: data.into(),
	}
}

/// Create a 3-topics log.
#[must_use]
pub fn log3(
	address: impl Into<H160>,
	topic0: impl Into<H256>,
	topic1: impl Into<H256>,
	topic2: impl Into<H256>,
	data: impl Into<Vec<u8>>,
) -> Log {
	Log {
		address: address.into(),
		topics: vec![topic0.into(), topic1.into(), topic2.into()],
		data: data.into(),
	}
}

/// Create a 4-topics log.
#[must_use]
pub fn log4(
	address: impl Into<H160>,
	topic0: impl Into<H256>,
	topic1: impl Into<H256>,
	topic2: impl Into<H256>,
	topic3: impl Into<H256>,
	data: impl Into<Vec<u8>>,
) -> Log {
	Log {
		address: address.into(),
		topics: vec![topic0.into(), topic1.into(), topic2.into(), topic3.into()],
		data: data.into(),
	}
}

/// Extension trait allowing to record logs into a PrecompileHandle.
pub trait LogExt {
	fn record(self, handle: &mut impl PrecompileHandle) -> EvmResult;

	fn compute_cost(&self) -> EvmResult<u64>;
}

impl LogExt for Log {
	fn record(self, handle: &mut impl PrecompileHandle) -> EvmResult {
		handle.log(self.address, self.topics, self.data)?;
		Ok(())
	}

	fn compute_cost(&self) -> EvmResult<u64> {
		crate::evm::costs::log_costs(self.topics.len(), self.data.len())
	}
}
