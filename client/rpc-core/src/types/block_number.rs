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

use std::fmt;

use ethereum_types::H256;
use serde::{
	de::{Error, MapAccess, Visitor},
	Deserialize, Deserializer, Serialize, Serializer,
};

/// Represents rpc api block number param.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Hash)]
pub enum BlockNumberOrHash {
	/// Hash
	Hash {
		/// block hash
		hash: H256,
		/// only return blocks part of the canon chain
		require_canonical: bool,
	},
	/// Number
	Num(u64),
	/// Latest block
	#[default]
	Latest,
	/// Earliest block (genesis)
	Earliest,
	/// Pending block (being mined)
	Pending,
	/// The most recent crypto-economically secure block.
	/// There is no difference between Ethereum's `safe` and `finalized`
	/// in Substrate finality gadget.
	Safe,
	/// The most recent crypto-economically secure block.
	Finalized,
}

impl<'a> Deserialize<'a> for BlockNumberOrHash {
	fn deserialize<D>(deserializer: D) -> Result<BlockNumberOrHash, D::Error>
	where
		D: Deserializer<'a>,
	{
		deserializer.deserialize_any(BlockNumberOrHashVisitor)
	}
}

impl BlockNumberOrHash {
	/// Convert block number to min block target.
	pub fn to_min_block_num(&self) -> Option<u64> {
		match *self {
			BlockNumberOrHash::Num(ref x) => Some(*x),
			BlockNumberOrHash::Earliest => Some(0),
			_ => None,
		}
	}
}

impl Serialize for BlockNumberOrHash {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			BlockNumberOrHash::Hash {
				hash,
				require_canonical,
			} => serializer.serialize_str(&format!(
				"{{ 'hash': '{}', 'requireCanonical': '{}'  }}",
				hash, require_canonical
			)),
			BlockNumberOrHash::Num(ref x) => serializer.serialize_str(&format!("0x{:x}", x)),
			BlockNumberOrHash::Latest => serializer.serialize_str("latest"),
			BlockNumberOrHash::Earliest => serializer.serialize_str("earliest"),
			BlockNumberOrHash::Pending => serializer.serialize_str("pending"),
			BlockNumberOrHash::Safe => serializer.serialize_str("safe"),
			BlockNumberOrHash::Finalized => serializer.serialize_str("finalized"),
		}
	}
}

struct BlockNumberOrHashVisitor;

impl<'a> Visitor<'a> for BlockNumberOrHashVisitor {
	type Value = BlockNumberOrHash;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		write!(
			formatter,
			"a block number or 'latest', 'safe', 'finalized', 'earliest' or 'pending'"
		)
	}

	fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
	where
		V: MapAccess<'a>,
	{
		let (mut require_canonical, mut block_number, mut block_hash) =
			(false, None::<u64>, None::<H256>);

		loop {
			let key_str: Option<String> = visitor.next_key()?;

			match key_str {
				Some(key) => match key.as_str() {
					"blockNumber" => {
						let value: String = visitor.next_value()?;
						if let Some(stripped) = value.strip_prefix("0x") {
							let number = u64::from_str_radix(stripped, 16).map_err(|e| {
								Error::custom(format!("Invalid block number: {}", e))
							})?;

							block_number = Some(number);
							break;
						} else {
							return Err(Error::custom(
								"Invalid block number: missing 0x prefix".to_string(),
							));
						}
					}
					"blockHash" => {
						block_hash = Some(visitor.next_value()?);
					}
					"requireCanonical" => {
						require_canonical = visitor.next_value()?;
					}
					key => return Err(Error::custom(format!("Unknown key: {}", key))),
				},
				None => break,
			};
		}

		if let Some(number) = block_number {
			return Ok(BlockNumberOrHash::Num(number));
		}

		if let Some(hash) = block_hash {
			return Ok(BlockNumberOrHash::Hash {
				hash,
				require_canonical,
			});
		}

		Err(Error::custom("Invalid input"))
	}

	fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
	where
		E: Error,
	{
		match value {
			"latest" => Ok(BlockNumberOrHash::Latest),
			"earliest" => Ok(BlockNumberOrHash::Earliest),
			"pending" => Ok(BlockNumberOrHash::Pending),
			"safe" => Ok(BlockNumberOrHash::Safe),
			"finalized" => Ok(BlockNumberOrHash::Finalized),
			_ if value.starts_with("0x") => u64::from_str_radix(&value[2..], 16)
				.map(BlockNumberOrHash::Num)
				.map_err(|e| Error::custom(format!("Invalid block number: {}", e))),
			_ => value
				.parse::<u64>()
				.map(BlockNumberOrHash::Num)
				.map_err(|_| {
					Error::custom(
						"Invalid block number: non-decimal or missing 0x prefix".to_string(),
					)
				}),
		}
	}

	fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
	where
		E: Error,
	{
		self.visit_str(value.as_ref())
	}

	fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
	where
		E: Error,
	{
		Ok(BlockNumberOrHash::Num(value))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn match_block_number(block_number: BlockNumberOrHash) -> Option<u64> {
		match block_number {
			BlockNumberOrHash::Num(number) => Some(number),
			BlockNumberOrHash::Earliest => Some(0),
			BlockNumberOrHash::Latest => Some(1000),
			BlockNumberOrHash::Safe => Some(999),
			BlockNumberOrHash::Finalized => Some(999),
			BlockNumberOrHash::Pending => Some(1001),
			_ => None,
		}
	}

	#[test]
	fn block_number_deserialize() {
		let bn_dec: BlockNumberOrHash = serde_json::from_str(r#""42""#).unwrap();
		let bn_hex: BlockNumberOrHash = serde_json::from_str(r#""0x45""#).unwrap();
		let bn_u64: BlockNumberOrHash = serde_json::from_str(r#"420"#).unwrap();
		let bn_tag_earliest: BlockNumberOrHash = serde_json::from_str(r#""earliest""#).unwrap();
		let bn_tag_latest: BlockNumberOrHash = serde_json::from_str(r#""latest""#).unwrap();
		let bn_tag_safe: BlockNumberOrHash = serde_json::from_str(r#""safe""#).unwrap();
		let bn_tag_finalized: BlockNumberOrHash = serde_json::from_str(r#""finalized""#).unwrap();
		let bn_tag_pending: BlockNumberOrHash = serde_json::from_str(r#""pending""#).unwrap();

		assert_eq!(match_block_number(bn_dec).unwrap(), 42);
		assert_eq!(match_block_number(bn_hex).unwrap(), 69);
		assert_eq!(match_block_number(bn_u64).unwrap(), 420);
		assert_eq!(match_block_number(bn_tag_earliest).unwrap(), 0);
		assert_eq!(match_block_number(bn_tag_latest).unwrap(), 1000);
		assert_eq!(match_block_number(bn_tag_safe).unwrap(), 999);
		assert_eq!(match_block_number(bn_tag_finalized).unwrap(), 999);
		assert_eq!(match_block_number(bn_tag_pending).unwrap(), 1001);
	}
}
