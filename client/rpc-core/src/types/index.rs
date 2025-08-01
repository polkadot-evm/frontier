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

impl Visitor<'_> for IndexVisitor {
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
