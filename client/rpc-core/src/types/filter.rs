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

use std::{
	collections::{BTreeMap, HashSet},
	sync::{Arc, Mutex},
};

use ethereum_types::{Bloom, BloomInput, H160, H256, U256};
use serde::{
	de::{DeserializeOwned, Error},
	Deserialize, Deserializer, Serialize, Serializer,
};
use serde_json::{from_value, Value};
use sp_core::{bounded_vec::BoundedVec, ConstU32};

use crate::types::{BlockNumberOrHash, Log};

const VARIADIC_MULTIPLE_MAX_SIZE: usize = 1024;

/// Variadic value
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum VariadicValue<T>
where
	T: DeserializeOwned,
{
	/// Single
	Single(T),
	/// List
	Multiple(Vec<T>),
	/// None
	Null,
}

impl<'a, T> Deserialize<'a> for VariadicValue<T>
where
	T: DeserializeOwned,
{
	fn deserialize<D>(deserializer: D) -> Result<VariadicValue<T>, D::Error>
	where
		D: Deserializer<'a>,
	{
		let v: Value = Deserialize::deserialize(deserializer)?;

		if v.is_null() {
			Ok(VariadicValue::Null)
		} else if let Ok(value) = from_value::<T>(v.clone()) {
			Ok(VariadicValue::Single(value))
		} else {
			match from_value::<Vec<T>>(v) {
				Ok(vec) => {
					if vec.len() <= VARIADIC_MULTIPLE_MAX_SIZE {
						Ok(VariadicValue::Multiple(vec))
					} else {
						Err(D::Error::custom(
							"Invalid variadic value type: too big array".to_string(),
						))
					}
				}
				Err(err) => Err(D::Error::custom(format!(
					"Invalid variadic value type: {err}"
				))),
			}
		}
	}
}

/// Filter Address
pub type FilterAddress = VariadicValue<H160>;
/// Topics are order-dependent. Each topic can also be an array of DATA with "or" options.
///
/// Example:
///
/// ```json
/// "topics": [
///     "0xddf252ad...",   // topic[0] must match `0xddf252ad...`, Event signature hash (e.g., Transfer(address,address,uint256))
///     "0xB",             // topic[1] must match `0xB`
///     null,              // topic[2] the null wildcard can be used to match anything on a topic position
///     ["0xC", "0xD"]     // topic[3] can be `0xC` OR `0xD`
/// ]
/// ```
///
pub type Topics = BoundedVec<VariadicValue<H256>, ConstU32<4>>;

pub type BloomFilter = Vec<Bloom>;

impl<T: AsRef<[u8]> + DeserializeOwned> From<&VariadicValue<T>> for BloomFilter {
	fn from(value: &VariadicValue<T>) -> Self {
		match value {
			VariadicValue::Single(item) => {
				let bloom: Bloom = BloomInput::Raw(item.as_ref()).into();
				vec![bloom]
			}
			VariadicValue::Multiple(items) => items
				.iter()
				.map(|item| BloomInput::Raw(item.as_ref()).into())
				.collect(),
			_ => vec![],
		}
	}
}

/// Filter
#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
	/// From Block
	pub from_block: Option<BlockNumberOrHash>,
	/// To Block
	pub to_block: Option<BlockNumberOrHash>,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Address
	pub address: Option<FilterAddress>,
	/// Topics
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub topics: Topics,
}

/// Helper for Filter matching.
/// Supports conditional indexed parameters and wildcards.
#[derive(Debug, Default)]
pub struct FilteredParams {
	pub filter: Filter,
}

impl FilteredParams {
	pub fn new(f: Filter) -> Self {
		FilteredParams { filter: f.clone() }
	}

	/// Build an address-based BloomFilter.
	pub fn address_bloom_filter(address: &Option<FilterAddress>) -> BloomFilter {
		if let Some(address) = address {
			return address.into();
		}
		Vec::new()
	}

	/// Build a topic-based BloomFilter.
	pub fn topics_bloom_filter(topics: &Topics) -> Vec<BloomFilter> {
		topics.into_iter().map(|topic| topic.into()).collect()
	}

	/// Evaluates if a Bloom contains a provided sequence of topics.
	pub fn topics_in_bloom(bloom: Bloom, topic_bloom_filters: &[BloomFilter]) -> bool {
		// Early return for empty filters - no constraints mean everything matches
		if topic_bloom_filters.is_empty() {
			return true;
		}

		// Each subset must match (AND condition between subsets)
		topic_bloom_filters.iter().all(|subset| {
			// Within each subset, any element can match (OR condition within subset)
			subset.is_empty()
				|| subset
					.iter()
					.any(|topic_bloom| bloom.contains_bloom(topic_bloom))
		})
	}

	/// Evaluates if a Bloom contains the provided address(es).
	pub fn address_in_bloom(bloom: Bloom, address_bloom_filter: &BloomFilter) -> bool {
		if address_bloom_filter.is_empty() {
			// No filter provided, match.
			return true;
		} else {
			// Wildcards are true.
			for el in address_bloom_filter {
				if bloom.contains_bloom(el) {
					return true;
				}
			}
		}
		false
	}

	/// Prepare the filter topics, taking into account wildcards.
	pub fn prepare_filter_wildcards(
		&self,
		topics: &[H256],
		input_topics: &Topics,
	) -> Vec<Vec<H256>> {
		let mut out: Vec<Vec<H256>> = Vec::new();
		for (idx, topic) in topics.iter().enumerate() {
			if let Some(t) = input_topics.get(idx) {
				match t {
					VariadicValue::Single(value) => {
						out.push(vec![*value]);
					}
					VariadicValue::Multiple(value) => {
						out.push(value.clone());
					}
					_ => {
						out.push(vec![*topic]);
					}
				};
			} else {
				out.push(vec![*topic]);
			}
		}

		out
	}

	pub fn filter_block_range(&self, block_number: u64) -> bool {
		let mut out = true;
		if let Some(BlockNumberOrHash::Num(from)) = self.filter.from_block {
			if from > block_number {
				out = false;
			}
		}
		if let Some(to) = self.filter.to_block {
			match to {
				BlockNumberOrHash::Num(to) => {
					if to < block_number {
						out = false;
					}
				}
				BlockNumberOrHash::Earliest => {
					out = false;
				}
				_ => {}
			}
		}
		out
	}

	pub fn filter_block_hash(&self, block_hash: H256) -> bool {
		if let Some(h) = self.filter.block_hash {
			if h != block_hash {
				return false;
			}
		}
		true
	}

	pub fn filter_address(&self, address: &H160) -> bool {
		if let Some(input_address) = &self.filter.clone().address {
			match input_address {
				VariadicValue::Single(x) => {
					if address != x {
						return false;
					}
				}
				VariadicValue::Multiple(x) => {
					if !x.contains(address) {
						return false;
					}
				}
				_ => {
					return true;
				}
			}
		}
		true
	}

	/// Returns true if the provided topics match the filter's topics.
	pub fn filter_topics(&self, topics: &[H256]) -> bool {
		// If the filter has more topics than the log, it can't match.
		if self.filter.topics.len() > topics.len() {
			return false;
		}
		let replaced = self.prepare_filter_wildcards(topics, &self.filter.topics);
		for (idx, topic) in topics.iter().enumerate() {
			if !replaced.get(idx).is_some_and(|v| v.contains(topic)) {
				return false;
			}
		}

		true
	}
}

/// Results of the filter_changes RPC.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilterChanges {
	/// New logs.
	Logs(Vec<Log>),
	/// New hashes (block or transactions)
	Hashes(Vec<H256>),
	/// Empty result,
	Empty,
}

impl Serialize for FilterChanges {
	fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			FilterChanges::Logs(ref logs) => logs.serialize(s),
			FilterChanges::Hashes(ref hashes) => hashes.serialize(s),
			FilterChanges::Empty => (&[] as &[Value]).serialize(s),
		}
	}
}

#[derive(Clone, Debug)]
pub enum FilterType {
	Block,
	PendingTransaction,
	Log(Filter),
}

#[derive(Clone, Debug)]
pub struct FilterPoolItem {
	pub last_poll: BlockNumberOrHash,
	pub filter_type: FilterType,
	pub at_block: u64,
	pub pending_transaction_hashes: HashSet<H256>,
}

/// On-memory stored filters created through the `eth_newFilter` RPC.
pub type FilterPool = Arc<Mutex<BTreeMap<U256, FilterPoolItem>>>;

#[cfg(test)]
mod tests {
	use super::*;
	use std::str::FromStr;

	fn block_bloom() -> Bloom {
		let test_address = H160::from_str("1000000000000000000000000000000000000000").unwrap();
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();

		let mut block_bloom = Bloom::default();
		block_bloom.accrue(BloomInput::Raw(&test_address[..]));
		block_bloom.accrue(BloomInput::Raw(&topic1[..]));
		block_bloom.accrue(BloomInput::Raw(&topic2[..]));
		block_bloom
	}

	#[test]
	fn bloom_filter_should_match_by_address() {
		let test_address = H160::from_str("1000000000000000000000000000000000000000").unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: Default::default(),
		};
		let address_bloom = FilteredParams::address_bloom_filter(&filter.address);
		assert!(FilteredParams::address_in_bloom(
			block_bloom(),
			&address_bloom
		));
	}

	#[test]
	fn bloom_filter_should_not_match_by_address() {
		let test_address = H160::from_str("2000000000000000000000000000000000000000").unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: Default::default(),
		};
		let address_bloom = FilteredParams::address_bloom_filter(&filter.address);
		assert!(!FilteredParams::address_in_bloom(
			block_bloom(),
			&address_bloom
		));
	}
	#[test]
	fn bloom_filter_should_match_by_topic() {
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: vec![
				VariadicValue::Single(topic1),
				VariadicValue::Multiple(vec![topic2, topic3]),
			]
			.try_into()
			.expect("qed"),
		};

		let topics_bloom = FilteredParams::topics_bloom_filter(&filter.topics);
		assert!(FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_not_match_by_topic() {
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("4000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("5000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: vec![
				VariadicValue::Single(topic1),
				VariadicValue::Multiple(vec![topic2, topic3]),
			]
			.try_into()
			.expect("qed"),
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&filter.topics);
		assert!(!FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_match_by_empty_topic() {
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: Default::default(),
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&filter.topics);
		assert!(FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_match_combined() {
		let test_address = H160::from_str("1000000000000000000000000000000000000000").unwrap();
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: vec![
				VariadicValue::Single(topic1),
				VariadicValue::Multiple(vec![topic2, topic3]),
			]
			.try_into()
			.expect("qed"),
		};
		let address_bloom = FilteredParams::address_bloom_filter(&filter.address);
		let topics_bloom = FilteredParams::topics_bloom_filter(&filter.topics);
		let matches = FilteredParams::address_in_bloom(block_bloom(), &address_bloom)
			&& FilteredParams::topics_in_bloom(block_bloom(), &topics_bloom);
		assert!(matches);
	}
	#[test]
	fn bloom_filter_should_not_match_combined() {
		let test_address = H160::from_str("2000000000000000000000000000000000000000").unwrap();
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: vec![
				VariadicValue::Single(topic1),
				VariadicValue::Multiple(vec![topic2, topic3]),
			]
			.try_into()
			.expect("qed"),
		};
		let address_bloom = FilteredParams::address_bloom_filter(&filter.address);
		let topics_bloom = FilteredParams::topics_bloom_filter(&filter.topics);
		let matches = FilteredParams::address_in_bloom(block_bloom(), &address_bloom)
			&& FilteredParams::topics_in_bloom(block_bloom(), &topics_bloom);
		assert!(!matches);
	}
	#[test]
	fn bloom_filter_should_match_wildcards_by_topic() {
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: vec![
				VariadicValue::Null,
				VariadicValue::Multiple(vec![topic2, topic3]),
			]
			.try_into()
			.expect("qed"),
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&filter.topics);
		assert!(FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_not_match_wildcards_by_topic() {
		let topic2 =
			H256::from_str("4000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("5000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: vec![
				VariadicValue::Null,
				VariadicValue::Multiple(vec![topic2, topic3]),
			]
			.try_into()
			.expect("qed"),
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&filter.topics);
		assert!(!FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}

	#[test]
	fn filter_topics_should_return_false_when_filter_has_more_topics_than_log() {
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: vec![VariadicValue::Null, VariadicValue::Single(topic2)]
				.try_into()
				.expect("qed"),
		};
		let filtered_params = FilteredParams::new(filter);
		// Expected not to match, as the filter has more topics than the log.
		assert!(!filtered_params.filter_topics(&vec![]));
		// Expected to match, as the first topic is a wildcard.
		assert!(filtered_params.filter_topics(&vec![topic1, topic2]));
	}
}
