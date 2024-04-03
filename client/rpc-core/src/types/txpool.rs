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

use std::collections::HashMap;

use ethereum::{TransactionAction, TransactionV2 as EthereumTransaction};
use ethereum_types::{H160, U256};
use serde::{Serialize, Serializer};

use crate::types::BuildFrom;

/// The entry maps an origin-address to a batch of scheduled transactions.
/// These batches themselves are maps associating nonces with actual transactions.
pub type TransactionMap<T> = HashMap<H160, HashMap<U256, T>>;

/// The result type of `txpool` API.
#[derive(Clone, Debug, Serialize)]
pub struct TxPoolResult<T: Serialize> {
	pub pending: T,
	pub queued: T,
}

/// The textual summary of all the transactions currently pending for inclusion in the next block(s).
#[derive(Clone, Debug)]
pub struct Summary {
	/// Recipient
	pub to: Option<H160>,
	/// Transferred value
	pub value: U256,
	/// Gas
	pub gas: U256,
	/// Gas Price
	pub gas_price: U256,
}

impl Serialize for Summary {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let res = format!(
			"0x{:x}: {} wei + {} gas x {} wei",
			self.to.unwrap_or_default(),
			self.value,
			self.gas,
			self.gas_price
		);
		serializer.serialize_str(&res)
	}
}

impl BuildFrom for Summary {
	fn build_from(_from: H160, transaction: &EthereumTransaction) -> Self {
		let (action, value, gas_price, gas) = match transaction {
			EthereumTransaction::Legacy(t) => (t.action, t.value, t.gas_price, t.gas_limit),
			EthereumTransaction::EIP2930(t) => (t.action, t.value, t.gas_price, t.gas_limit),
			EthereumTransaction::EIP1559(t) => (t.action, t.value, t.max_fee_per_gas, t.gas_limit),
		};
		Self {
			to: match action {
				TransactionAction::Call(to) => Some(to),
				_ => None,
			},
			value,
			gas_price,
			gas,
		}
	}
}
