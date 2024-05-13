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

use ethereum_types::H256;
use serde::{de, Deserialize, Serialize};

use crate::{block::Header, filter::Filter, log::Log, transaction::Transaction};

/// Subscription kind.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PubSubKind {
	/// New block headers subscription.
	NewHeads,
	/// Logs subscription.
	Logs,
	/// New Pending Transactions subscription.
	NewPendingTransactions,
}

/// Any additional parameters for a subscription.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum PubSubParams {
	/// No parameters passed.
	#[default]
	None,
	/// Log parameters.
	Logs(Box<Filter>),
	/// Boolean parameter for new pending transactions.
	Bool(bool),
}

impl serde::Serialize for PubSubParams {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::None => serializer.serialize_none(),
			Self::Logs(logs) => logs.serialize(serializer),
			Self::Bool(full) => full.serialize(serializer),
		}
	}
}

impl<'de> serde::Deserialize<'de> for PubSubParams {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let v = serde_json::Value::deserialize(deserializer)?;

		if v.is_null() {
			return Ok(Self::None);
		}

		if let Some(val) = v.as_bool() {
			return Ok(Self::Bool(val));
		}

		serde_json::from_value(v)
			.map(|f| Self::Logs(Box::new(f)))
			.map_err(|e| de::Error::custom(format!("Invalid Pub-Sub parameters: {e}")))
	}
}

/// Subscription result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PubSubResult {
	/// New block header.
	Header(Box<Header>),
	/// Log.
	Log(Box<Log>),
	/// Transaction hash.
	TransactionHash(H256),
	/// Transaction.
	FullTransaction(Box<Transaction>),
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn pubsub_params_serde_impl() {
		let cases = [
			("null", PubSubParams::None),
			("true", PubSubParams::Bool(true)),
			("false", PubSubParams::Bool(false)),
		];
		for (raw, typed) in cases {
			let deserialized = serde_json::from_str::<PubSubParams>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw);
		}
	}
}
