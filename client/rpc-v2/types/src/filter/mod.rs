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

mod block_option;
mod utility;

use std::fmt;

use ethereum_types::{Address, H256};
use serde::{
	de::{self, MapAccess, Visitor},
	ser::SerializeStruct,
	Deserialize,
};

pub use self::{
	block_option::FilterBlockOption,
	utility::{FilterSet, ValueOrArray},
};
use crate::{block_id::BlockNumberOrTag, log::Log};

/// The maximum number of topics supported in [`Filter`].
pub const MAX_TOPICS: usize = 4;

pub type AddressFilter = FilterSet<Address>;
pub type TopicFilter = FilterSet<H256>;

/// Filter parameters of `eth_newFilter` and `eth_getLogs` RPC.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Filter {
	/// Filter block options, specifying on which blocks the filter should match.
	pub block_option: FilterBlockOption,
	/// Address filter.
	pub address: Option<AddressFilter>,
	/// Topics filter.
	pub topics: Option<[TopicFilter; MAX_TOPICS]>,
}

impl Filter {
	/// Creates a new, empty filter.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the block number this range filter should start at.
	#[allow(clippy::wrong_self_convention)]
	pub fn from_block<T: Into<BlockNumberOrTag>>(mut self, block: T) -> Self {
		self.block_option = self.block_option.from_block(block.into());
		self
	}

	/// Sets the block number this range filter should end at.
	#[allow(clippy::wrong_self_convention)]
	pub fn to_block<T: Into<BlockNumberOrTag>>(mut self, block: T) -> Self {
		self.block_option = self.block_option.to_block(block.into());
		self
	}

	/// Pins the block hash this filter should target.
	pub fn at_block_hash<T: Into<H256>>(mut self, hash: T) -> Self {
		self.block_option = FilterBlockOption::block_hash(hash.into());
		self
	}

	/// Sets the address filter.
	pub fn address<T: Into<ValueOrArray<Address>>>(mut self, address: T) -> Self {
		self.address = Some(address.into().into());
		self
	}

	/// Sets event_signature(topic0) (the event name for non-anonymous events).
	pub fn event_signature<T: Into<TopicFilter>>(self, topic: T) -> Self {
		self.topic(0, topic)
	}

	/// Sets the 1st indexed topic.
	pub fn topic1<T: Into<TopicFilter>>(self, topic: T) -> Self {
		self.topic(1, topic)
	}

	/// Sets the 2nd indexed topic.
	pub fn topic2<T: Into<TopicFilter>>(self, topic: T) -> Self {
		self.topic(2, topic)
	}

	/// Sets the 3rd indexed topic.
	pub fn topic3<T: Into<TopicFilter>>(self, topic: T) -> Self {
		self.topic(3, topic)
	}

	fn topic<T: Into<TopicFilter>>(mut self, index: usize, topic: T) -> Self {
		match &mut self.topics {
			Some(topics) => {
				topics[index] = topic.into();
			}
			None => {
				let mut topics: [TopicFilter; MAX_TOPICS] = Default::default();
				topics[index] = topic.into();
				self.topics = Some(topics);
			}
		}
		self
	}
}

type RawAddressFilter = ValueOrArray<Address>;
type RawTopicsFilter = Vec<Option<ValueOrArray<H256>>>;

impl serde::Serialize for Filter {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let mut s = match &self.block_option {
			FilterBlockOption::BlockNumberRange {
				from_block,
				to_block,
			} => {
				let mut s = serializer.serialize_struct("Filter", 2 + 1 + 1)?;
				s.serialize_field("fromBlock", from_block)?;
				s.serialize_field("toBlock", to_block)?;
				s
			}
			FilterBlockOption::BlockHashAt { block_hash } => {
				let mut s = serializer.serialize_struct("Filter", 1 + 1 + 1)?;
				s.serialize_field("blockHash", block_hash)?;
				s
			}
		};

		match &self.address {
			Some(address) => s.serialize_field("address", &address.to_value_or_array())?,
			None => s.serialize_field("address", &Option::<RawAddressFilter>::None)?,
		}

		match &self.topics {
			Some(topics) => {
				let mut filtered_topics = Vec::new();

				let mut filtered_topics_len = 0;
				for (idx, topic) in topics.iter().enumerate() {
					if !topic.is_empty() {
						filtered_topics_len = idx + 1;
					}
					filtered_topics.push(topic.to_value_or_array());
				}
				filtered_topics.truncate(filtered_topics_len);

				s.serialize_field("topics", &filtered_topics)?;
			}
			None => s.serialize_field("topics", &Option::<RawTopicsFilter>::None)?,
		}

		s.end()
	}
}

impl<'de> serde::Deserialize<'de> for Filter {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct FilterVisitor;

		impl<'de> Visitor<'de> for FilterVisitor {
			type Value = Filter;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("Filter object")
			}

			fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
			where
				A: MapAccess<'de>,
			{
				let mut from_block: Option<Option<BlockNumberOrTag>> = None;
				let mut to_block: Option<Option<BlockNumberOrTag>> = None;
				let mut block_hash: Option<Option<H256>> = None;
				let mut address: Option<Option<RawAddressFilter>> = None;
				let mut topics: Option<Option<RawTopicsFilter>> = None;

				while let Some(key) = map.next_key::<String>()? {
					match key.as_str() {
						"fromBlock" => {
							if from_block.is_some() {
								return Err(de::Error::duplicate_field("fromBlock"));
							}
							if block_hash.is_some() {
								return Err(de::Error::custom(
									"fromBlock not allowed with blockHash",
								));
							}
							from_block = Some(map.next_value()?)
						}
						"toBlock" => {
							if to_block.is_some() {
								return Err(de::Error::duplicate_field("toBlock"));
							}
							if block_hash.is_some() {
								return Err(de::Error::custom(
									"toBlock not allowed with blockHash",
								));
							}
							to_block = Some(map.next_value()?)
						}
						"blockHash" => {
							if block_hash.is_some() {
								return Err(de::Error::duplicate_field("blockHash"));
							}
							if from_block.is_some() || to_block.is_some() {
								return Err(de::Error::custom(
									"fromBlock,toBlock not allowed with blockHash",
								));
							}
							block_hash = Some(map.next_value()?)
						}
						"address" => {
							if address.is_some() {
								return Err(de::Error::duplicate_field("address"));
							}
							address = Some(map.next_value()?)
						}
						"topics" => {
							if topics.is_some() {
								return Err(de::Error::duplicate_field("topics"));
							}
							topics = Some(map.next_value()?)
						}
						key => {
							return Err(de::Error::unknown_field(
								key,
								&["fromBlock", "toBlock", "blockHash", "address", "topics"],
							))
						}
					}
				}

				let from_block = from_block.unwrap_or_default();
				let to_block = to_block.unwrap_or_default();
				let block_hash = block_hash.unwrap_or_default();

				let block_option = if let Some(block_hash) = block_hash {
					FilterBlockOption::BlockHashAt { block_hash }
				} else {
					FilterBlockOption::BlockNumberRange {
						from_block,
						to_block,
					}
				};

				let address = address.flatten().map(FilterSet::from);

				let topics = match topics.flatten() {
					Some(topics_vec) => {
						if topics_vec.len() > MAX_TOPICS {
							return Err(de::Error::custom("exceeded maximum topics"));
						}

						let mut topics: [TopicFilter; MAX_TOPICS] = Default::default();
						for (idx, topic) in topics_vec.into_iter().enumerate() {
							topics[idx] = topic.map(FilterSet::from).unwrap_or_default();
						}
						Some(topics)
					}
					None => None,
				};

				Ok(Filter {
					block_option,
					address,
					topics,
				})
			}
		}

		deserializer.deserialize_any(FilterVisitor)
	}
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum FilterChanges {
	/// Empty result.
	#[default]
	Empty,
	/// New logs.
	Logs(Vec<Log>),
	/// New hashes (block or transactions).
	Hashes(Vec<H256>),
}

impl serde::Serialize for FilterChanges {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::Empty => (&[] as &[()]).serialize(serializer),
			Self::Logs(logs) => logs.serialize(serializer),
			Self::Hashes(hashes) => hashes.serialize(serializer),
		}
	}
}

impl<'de> serde::Deserialize<'de> for FilterChanges {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		#[derive(Deserialize)]
		#[serde(untagged)]
		enum Changes {
			Logs(Vec<Log>),
			Hashes(Vec<H256>),
		}

		let changes = Changes::deserialize(deserializer)?;
		Ok(match changes {
			Changes::Logs(logs) => {
				if logs.is_empty() {
					Self::Empty
				} else {
					Self::Logs(logs)
				}
			}
			Changes::Hashes(hashes) => {
				if hashes.is_empty() {
					Self::Empty
				} else {
					Self::Hashes(hashes)
				}
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn h256(s: &str) -> H256 {
		s.parse().unwrap()
	}

	fn address(s: &str) -> Address {
		s.parse().unwrap()
	}

	#[test]
	fn filter_serde_impl() {
		let valid_cases = [
			(
				r#"{
					"fromBlock":"earliest",
					"toBlock":null,
					"address":null,
					"topics":null
				}"#,
				Filter::default().from_block(BlockNumberOrTag::Earliest),
			),
			(
				r#"{
					"fromBlock":"earliest",
					"toBlock":null,
					"address":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
					"topics":null
				}"#,
				Filter::default()
					.from_block(BlockNumberOrTag::Earliest)
					.address(address("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")),
			),
			(
				r#"{
					"blockHash":"0x1111111111111111111111111111111111111111111111111111111111111111",
					"address":[
						"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
						"0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
					],
					"topics":null
				}"#,
				Filter::default()
					.at_block_hash(h256(
						"0x1111111111111111111111111111111111111111111111111111111111111111",
					))
					.address(vec![
						address("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
						address("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
					]),
			),
		];

		for (raw, typed) in valid_cases {
			let deserialized = serde_json::from_str::<Filter>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw.split_whitespace().collect::<String>());
		}
	}

	#[test]
	fn filter_changes_serde_impl() {
		let cases = [
			(r#"[]"#, FilterChanges::Empty),
			(
				r#"[
					"0x1111111111111111111111111111111111111111111111111111111111111111",
					"0x2222222222222222222222222222222222222222222222222222222222222222"
				]"#,
				FilterChanges::Hashes(vec![
					h256("0x1111111111111111111111111111111111111111111111111111111111111111"),
					h256("0x2222222222222222222222222222222222222222222222222222222222222222"),
				]),
			),
		];

		for (raw, typed) in cases {
			let deserialized = serde_json::from_str::<FilterChanges>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw.split_whitespace().collect::<String>());
		}
	}
}
