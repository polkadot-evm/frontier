// Copyright 2015-2020 Parity Technologies (UK) Ltd.
// This file is part of Frontier.

// Open Ethereum is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Open Ethereum is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Open Ethereum.  If not, see <http://www.gnu.org/licenses/>.

use ethereum_types::{H160, H256};
use serde::de::{Error, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, from_value};

use crate::types::{BlockNumber, Log};

/// Variadic value
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum VariadicValue<T> where T: DeserializeOwned {
	/// Single
	Single(T),
	/// List
	Multiple(Vec<T>),
	/// None
	Null,
}

impl<'a, T> Deserialize<'a> for VariadicValue<T> where T: DeserializeOwned {
	fn deserialize<D>(deserializer: D) -> Result<VariadicValue<T>, D::Error>
	where D: Deserializer<'a> {
		let v: Value = Deserialize::deserialize(deserializer)?;

		if v.is_null() {
			return Ok(VariadicValue::Null);
		}

		from_value(v.clone()).map(VariadicValue::Single)
			.or_else(|_| from_value(v).map(VariadicValue::Multiple))
			.map_err(|err| D::Error::custom(format!("Invalid variadic value type: {}", err)))
	}
}

/// Filter Address
pub type FilterAddress = VariadicValue<H160>;
/// Topic
pub type Topic = VariadicValue<H256>;

/// Filter
#[derive(Debug, PartialEq, Clone, Deserialize, Eq, Hash)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
	/// From Block
	pub from_block: Option<BlockNumber>,
	/// To Block
	pub to_block: Option<BlockNumber>,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Address
	pub address: Option<FilterAddress>,
	/// Topics
	pub topics: Option<Topic>,
}

/// Results of the filter_changes RPC.
#[derive(Debug, PartialEq)]
pub enum FilterChanges {
	/// New logs.
	Logs(Vec<Log>),
	/// New hashes (block or transactions)
	Hashes(Vec<H256>),
	/// Empty result,
	Empty,
}

impl Serialize for FilterChanges {
	fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error> where S: Serializer {
		match *self {
			FilterChanges::Logs(ref logs) => logs.serialize(s),
			FilterChanges::Hashes(ref hashes) => hashes.serialize(s),
			FilterChanges::Empty => (&[] as &[Value]).serialize(s),
		}
	}
}
