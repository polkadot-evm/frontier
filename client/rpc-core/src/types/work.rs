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

use ethereum_types::{H256, U256};
use serde::{Serialize, Serializer};

/// The result of an `eth_getWork` call: it differs based on an option
/// whether to send the block number.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Work {
	/// The proof-of-work hash.
	pub pow_hash: H256,
	/// The seed hash.
	pub seed_hash: H256,
	/// The target.
	pub target: H256,
	/// The block number: this isn't always stored.
	pub number: Option<u64>,
}

impl Serialize for Work {
	fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match self.number.as_ref() {
			Some(num) => (
				&self.pow_hash,
				&self.seed_hash,
				&self.target,
				U256::from(*num),
			)
				.serialize(s),
			None => (&self.pow_hash, &self.seed_hash, &self.target).serialize(s),
		}
	}
}
