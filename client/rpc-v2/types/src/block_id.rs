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

use std::{fmt, str};

use ethereum_types::H256;
use serde::{Deserialize, Serialize};

/// A Block identifier.
/// <https://github.com/ethereum/EIPs/blob/master/EIPS/eip-1898.md>
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BlockNumberOrTagOrHash {
	/// A block number or tag.
	Number(BlockNumberOrTag),
	/// A block hash and an optional indication if it's canonical.
	Hash(BlockHash),
}

impl Default for BlockNumberOrTagOrHash {
	fn default() -> Self {
		Self::Number(BlockNumberOrTag::default())
	}
}

impl From<BlockNumberOrTag> for BlockNumberOrTagOrHash {
	fn from(value: BlockNumberOrTag) -> Self {
		Self::Number(value)
	}
}

impl From<u64> for BlockNumberOrTagOrHash {
	fn from(value: u64) -> Self {
		Self::Number(BlockNumberOrTag::Number(value))
	}
}

impl From<BlockHash> for BlockNumberOrTagOrHash {
	fn from(value: BlockHash) -> Self {
		Self::Hash(value)
	}
}

impl serde::Serialize for BlockNumberOrTagOrHash {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::Number(number) => number.serialize(serializer),
			Self::Hash(hash) => hash.serialize(serializer),
		}
	}
}

impl<'de> serde::Deserialize<'de> for BlockNumberOrTagOrHash {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		use serde::de;

		struct BlockNumberOrTagOrHashVisitor;

		impl<'de> de::Visitor<'de> for BlockNumberOrTagOrHashVisitor {
			type Value = BlockNumberOrTagOrHash;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("Block number or hash parameter that following EIP-1898")
			}

			fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				// There is no way to clearly distinguish between a DATA parameter and a QUANTITY parameter.
				// However, since the hex string should be a QUANTITY, we can safely assume that if the len is 66 bytes, it is in fact a hash,
				if v.len() == 66 {
					let hash = v.parse::<H256>().map_err(de::Error::custom)?;
					Ok(BlockNumberOrTagOrHash::Hash(hash.into()))
				} else {
					// quantity hex string or tag
					let number = v.parse::<BlockNumberOrTag>().map_err(de::Error::custom)?;
					Ok(BlockNumberOrTagOrHash::Number(number))
				}
			}

			fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
			where
				A: de::MapAccess<'de>,
			{
				let mut number = None;
				let mut block_hash = None;
				let mut require_canonical = None;

				while let Some(key) = map.next_key::<String>()? {
					match key.as_str() {
						"blockNumber" => {
							if number.is_some() || block_hash.is_some() {
								return Err(de::Error::duplicate_field("blockNumber"));
							}
							if require_canonical.is_some() {
								return Err(de::Error::custom("Non-valid require_canonical field"));
							}
							number = Some(map.next_value::<BlockNumberOrTag>()?)
						}
						"blockHash" => {
							if number.is_some() || block_hash.is_some() {
								return Err(de::Error::duplicate_field("blockHash"));
							}
							block_hash = Some(map.next_value::<H256>()?);
						}
						"requireCanonical" => {
							if number.is_some() || require_canonical.is_some() {
								return Err(de::Error::duplicate_field("requireCanonical"));
							}
							require_canonical = Some(map.next_value::<bool>()?)
						}
						key => {
							return Err(de::Error::unknown_field(
								key,
								&["blockNumber", "blockHash", "requireCanonical"],
							))
						}
					}
				}

				if let Some(number) = number {
					Ok(BlockNumberOrTagOrHash::Number(number))
				} else if let Some(block_hash) = block_hash {
					Ok(BlockNumberOrTagOrHash::Hash(BlockHash {
						block_hash,
						require_canonical,
					}))
				} else {
					Err(de::Error::custom(
						"Expected `blockNumber` or `blockHash` with `requireCanonical` optionally",
					))
				}
			}
		}

		deserializer.deserialize_any(BlockNumberOrTagOrHashVisitor)
	}
}

/// A block number or tag ("latest", "earliest", "pending", "finalized", "safe").
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Hash)]
pub enum BlockNumberOrTag {
	/// Latest block.
	///
	/// The most recent block in the canonical chain observed by the client, this block may be
	/// re-organized out of the canonical chain even under healthy/normal condition.
	#[default]
	Latest,
	/// Finalized block accepted as canonical.
	///
	/// The most recent crypto-economically secure block, cannot be re-organized outside manual
	/// intervention driven by community coordination.
	Finalized,
	/// Safe head block.
	///
	/// The most recent block that is safe from re-organized under honest majority and certain
	/// synchronicity assumptions.
	///
	/// There is no difference between Ethereum's `safe` and `finalized` in Substrate finality gadget
	Safe,
	/// Earliest block (genesis).
	///
	/// The lowest numbered block the client has available.
	Earliest,
	/// Pending block (being mined).
	///
	/// A sample next block built by the client on top of `latest` and containing the set of
	/// transactions usually taken from local txpool.
	Pending,
	/// Block number.
	Number(u64),
}

impl From<u64> for BlockNumberOrTag {
	fn from(value: u64) -> Self {
		Self::Number(value)
	}
}

impl str::FromStr for BlockNumberOrTag {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(match s {
			"latest" => Self::Latest,
			"finalized" => Self::Finalized,
			"safe" => Self::Safe,
			"earliest" => Self::Earliest,
			"pending" => Self::Pending,
			_number => {
				if let Some(hex_val) = s.strip_prefix("0x") {
					let number = u64::from_str_radix(hex_val, 16).map_err(|err| err.to_string())?;
					BlockNumberOrTag::Number(number)
				} else {
					return Err("hex string without 0x prefix".to_string());
				}
			}
		})
	}
}

impl serde::Serialize for BlockNumberOrTag {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::Latest => serializer.serialize_str("latest"),
			Self::Finalized => serializer.serialize_str("finalized"),
			Self::Safe => serializer.serialize_str("safe"),
			Self::Earliest => serializer.serialize_str("earliest"),
			Self::Pending => serializer.serialize_str("pending"),
			Self::Number(num) => serializer.serialize_str(&format!("0x{num:x}")),
		}
	}
}

impl<'de> serde::Deserialize<'de> for BlockNumberOrTag {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?.to_lowercase();
		s.parse().map_err(serde::de::Error::custom)
	}
}

/// A block hash which may have a boolean `requireCanonical` field.
///
/// If it's false, an RPC call should raise if a block matching the hash is not found.
/// If it's true, an RPC call should additionally raise if the block is not in the canonical chain.
///
/// <https://github.com/ethereum/EIPs/blob/master/EIPS/eip-1898.md#specification>
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockHash {
	/// A block hash.
	block_hash: H256,
	/// The indication if the block hash is canonical.
	#[serde(skip_serializing_if = "Option::is_none")]
	require_canonical: Option<bool>,
}

impl From<H256> for BlockHash {
	fn from(value: H256) -> Self {
		Self {
			block_hash: value,
			require_canonical: None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn eip1898_block_number_serde_impl() {
		let cases = [
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Latest),
				serde_json::json!("latest"),
				serde_json::json!({ "blockNumber": "latest" }),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Finalized),
				serde_json::json!("finalized"),
				serde_json::json!({ "blockNumber": "finalized" }),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Safe),
				serde_json::json!("safe"),
				serde_json::json!({ "blockNumber": "safe" }),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Earliest),
				serde_json::json!("earliest"),
				serde_json::json!({ "blockNumber": "earliest" }),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Pending),
				serde_json::json!("pending"),
				serde_json::json!({ "blockNumber": "pending" }),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Number(0)),
				serde_json::json!("0x0"),
				serde_json::json!("0x0"),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Number(0)),
				serde_json::json!("0x0"),
				serde_json::json!({ "blockNumber": "0x0" }),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Number(255)),
				serde_json::json!("0xff"),
				serde_json::json!("0xff"),
			),
			(
				BlockNumberOrTagOrHash::Number(BlockNumberOrTag::Number(255)),
				serde_json::json!("0xff"),
				serde_json::json!({ "blockNumber": "0xff" }),
			),
		];
		for (block_number, ser, de) in cases {
			assert_eq!(serde_json::to_value(block_number).unwrap(), ser);
			assert_eq!(
				serde_json::from_value::<BlockNumberOrTagOrHash>(de).unwrap(),
				block_number
			);
		}
	}

	#[test]
	fn eip1898_block_hash_serde_impl() {
		let cases = [
			(
				BlockNumberOrTagOrHash::Hash(BlockHash {
					block_hash:
						"0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
							.parse()
							.unwrap(),
					require_canonical: None,
				}),
				serde_json::json!({ "blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3" }),
				serde_json::json!(
					"0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
				),
			),
			(
				BlockNumberOrTagOrHash::Hash(BlockHash {
					block_hash:
						"0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
							.parse()
							.unwrap(),
					require_canonical: None,
				}),
				serde_json::json!({ "blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3" }),
				serde_json::json!({ "blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3" }),
			),
			(
				BlockNumberOrTagOrHash::Hash(BlockHash {
					block_hash:
						"0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
							.parse()
							.unwrap(),
					require_canonical: Some(true),
				}),
				serde_json::json!({
					"blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3",
					"requireCanonical": true
				}),
				serde_json::json!({
					"blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3",
					"requireCanonical": true
				}),
			),
			(
				BlockNumberOrTagOrHash::Hash(BlockHash {
					block_hash:
						"0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
							.parse()
							.unwrap(),
					require_canonical: Some(false),
				}),
				serde_json::json!({
					"blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3",
					"requireCanonical": false
				}),
				serde_json::json!({
					"blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3",
					"requireCanonical": false
				}),
			),
		];
		for (block_hash, ser, de) in cases {
			assert_eq!(serde_json::to_value(block_hash).unwrap(), ser);
			assert_eq!(
				serde_json::from_value::<BlockNumberOrTagOrHash>(de).unwrap(),
				block_hash
			);
		}
	}

	#[test]
	fn invalid_eip1898_block_parameter_deserialization() {
		let invalid_cases = [
			serde_json::json!(0),
			serde_json::json!({ "blockNumber": "0" }),
			serde_json::json!({ "blockNumber": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3" }),
			serde_json::json!({ "blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa" }),
			serde_json::json!({
				"blockNumber": "0x00",
				"blockHash": "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3",
				"requireCanonical": false
			}),
		];
		for case in invalid_cases {
			let res = serde_json::from_value::<BlockNumberOrTagOrHash>(case);
			// println!("{res:?}");
			assert!(res.is_err());
		}
	}
}
