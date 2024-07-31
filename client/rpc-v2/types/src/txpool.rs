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

use std::{collections::BTreeMap, fmt};

use ethereum_types::{Address, U256, U64};
use serde::{de, Deserialize, Serialize};

use crate::transaction::Transaction;

pub type TxpoolInspect = TxpoolResult<AddressMapping<NonceMapping<Summary>>>;
pub type TxpoolContent = TxpoolResult<AddressMapping<NonceMapping<Transaction>>>;
pub type TxpoolContentFrom = TxpoolResult<NonceMapping<Transaction>>;
pub type TxpoolStatus = TxpoolResult<U64>;

pub type NonceMapping<T> = BTreeMap<u64, T>;
pub type AddressMapping<T> = BTreeMap<Address, T>;

/// The txpool result type.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxpoolResult<T> {
	/// Pending transactions.
	pub pending: T,
	/// Queued transactions.
	pub queued: T,
}

/// The textual summary of all the transactions currently pending for inclusion in the next block(s).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Summary {
	/// Recipient.
	pub to: Option<Address>,
	/// Transferred value.
	pub value: U256,
	/// Gas limit.
	pub gas: u128,
	/// Gas price.
	pub gas_price: u128,
}

impl serde::Serialize for Summary {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let formatted_to = if let Some(to) = self.to {
			format!("{to:?}")
		} else {
			"contract creation".to_string()
		};
		let formatted = format!(
			"{}: {} wei + {} gas × {} wei",
			formatted_to, self.value, self.gas, self.gas_price
		);
		serializer.serialize_str(&formatted)
	}
}

impl<'de> serde::Deserialize<'de> for Summary {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct SummaryVisitor;
		impl<'de> de::Visitor<'de> for SummaryVisitor {
			type Value = Summary;

			fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
				formatter.write_str("{{to}}: {{value}} wei + {{gas}} gas × {{gas_price}} wei")
			}

			fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				let addr_split: Vec<&str> = v.split(": ").collect();
				if addr_split.len() != 2 {
					return Err(de::Error::custom("invalid `to` format"));
				}

				let value_split: Vec<&str> = addr_split[1].split(" wei + ").collect();
				if value_split.len() != 2 {
					return Err(de::Error::custom("invalid `value` format"));
				}

				let gas_split: Vec<&str> = value_split[1].split(" gas × ").collect();
				if gas_split.len() != 2 {
					return Err(de::Error::custom("invalid `gas` format"));
				}

				let gas_price_split: Vec<&str> = gas_split[1].split(" wei").collect();
				if gas_price_split.len() != 2 {
					return Err(de::Error::custom("invalid `gas_price` format"));
				}

				let to = match addr_split[0] {
					"contract creation" => None,
					addr => {
						let addr = addr
							.trim_start_matches("0x")
							.parse::<Address>()
							.map_err(de::Error::custom)?;
						Some(addr)
					}
				};
				let value = U256::from_dec_str(value_split[0]).map_err(de::Error::custom)?;
				let gas = gas_split[0].parse::<u128>().map_err(de::Error::custom)?;
				let gas_price = gas_price_split[0]
					.parse::<u128>()
					.map_err(de::Error::custom)?;

				Ok(Summary {
					to,
					value,
					gas,
					gas_price,
				})
			}

			fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				self.visit_str(&v)
			}
		}

		deserializer.deserialize_str(SummaryVisitor)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn inspect_summary_serde_impl() {
		let valid_cases = [
			(
				r#""contract creation: 2472666000 wei + 21000 gas × 1000 wei""#,
				Summary {
					to: None,
					value: U256::from(2472666000u64),
					gas: 21000,
					gas_price: 1000,
				},
			),
			(
				r#""0x1111111111111111111111111111111111111111: 2472666000 wei + 21000 gas × 1000 wei""#,
				Summary {
					to: Some(
						"0x1111111111111111111111111111111111111111"
							.parse::<Address>()
							.unwrap(),
					),
					value: U256::from(2472666000u64),
					gas: 21000,
					gas_price: 1000,
				},
			),
		];
		for (raw, typed) in valid_cases {
			let deserialized = serde_json::from_str::<Summary>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw);
		}

		let invalid_cases = [
			r#"": ""#,
			r#"" : 2472666000 wei + 21000 gas × 1000 wei""#,
			r#""0x: 2472666000 wei + 21000 gas × 1000 wei""#,
			r#""0x1111111111111111111111111111111111111111: 2472666000 wei""#,
			r#""0x1111111111111111111111111111111111111111: 2472666000 wei + 21000 gas × ""#,
			r#""0x1111111111111111111111111111111111111111: 2472666000 wei + 21000 gas × 1000""#,
		];
		for raw in invalid_cases {
			let summary: Result<Summary, _> = serde_json::from_str(raw);
			assert!(summary.is_err());
		}
	}
}
