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

use std::fmt;

use serde::{
	de::{Error, Visitor},
	Deserialize, Deserializer,
};

/// Represents usize.
#[derive(Debug, Eq, PartialEq)]
pub struct Index(usize);

impl Index {
	/// Convert to usize
	pub fn value(&self) -> usize {
		self.0
	}
}

impl<'a> Deserialize<'a> for Index {
	fn deserialize<D>(deserializer: D) -> Result<Index, D::Error>
	where
		D: Deserializer<'a>,
	{
		deserializer.deserialize_any(IndexVisitor)
	}
}

struct IndexVisitor;

impl<'a> Visitor<'a> for IndexVisitor {
	type Value = Index;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		write!(formatter, "a hex-encoded or decimal index")
	}

	fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
	where
		E: Error,
	{
		match value {
			_ if value.starts_with("0x") => usize::from_str_radix(&value[2..], 16)
				.map(Index)
				.map_err(|e| Error::custom(format!("Invalid index: {}", e))),
			_ => value
				.parse::<usize>()
				.map(Index)
				.map_err(|e| Error::custom(format!("Invalid index: {}", e))),
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
		Ok(Index(value as usize))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json;

	#[test]
	fn index_deserialization() {
		let s = r#"["0xa", "10", 42]"#;
		let deserialized: Vec<Index> = serde_json::from_str(s).unwrap();
		assert_eq!(deserialized, vec![Index(10), Index(10), Index(42)]);
	}
}
