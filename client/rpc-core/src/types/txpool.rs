// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2022 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
