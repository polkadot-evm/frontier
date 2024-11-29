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

#[derive(Copy, Clone, PartialEq, Eq, Default, Hash)]
pub struct Index(usize);

impl fmt::Debug for Index {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::LowerHex::fmt(&self.0, f)
	}
}

impl fmt::Display for Index {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::LowerHex::fmt(&self.0, f)
	}
}

impl From<Index> for usize {
	fn from(idx: Index) -> Self {
		idx.0
	}
}

impl From<usize> for Index {
	fn from(value: usize) -> Self {
		Self(value)
	}
}

impl serde::Serialize for Index {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		serializer.serialize_str(&format!("0x{:x}", self.0))
	}
}

impl<'de> serde::Deserialize<'de> for Index {
	fn deserialize<D>(deserializer: D) -> Result<Index, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		use serde::de;

		struct IndexVisitor;

		impl<'de> de::Visitor<'de> for IndexVisitor {
			type Value = Index;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("hex-encoded or decimal index")
			}

			fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(Index(value as usize))
			}

			fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				if let Some(val) = value.strip_prefix("0x") {
					usize::from_str_radix(val, 16)
						.map(Index)
						.map_err(de::Error::custom)
				} else {
					value.parse::<usize>().map(Index).map_err(de::Error::custom)
				}
			}

			fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				self.visit_str(value.as_ref())
			}
		}

		deserializer.deserialize_any(IndexVisitor)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn index_serialize() {
		let indexes = vec![Index(10), Index(10), Index(u32::MAX as usize)];
		let serialized = serde_json::to_string(&indexes).unwrap();
		let expected = r#"["0xa","0xa","0xffffffff"]"#;
		assert_eq!(serialized, expected);
	}

	#[test]
	fn index_deserialize() {
		let s = r#"["0xa", "10", "0xffffffff"]"#;
		let deserialized: Vec<Index> = serde_json::from_str(s).unwrap();
		let expected = vec![Index(10), Index(10), Index(u32::MAX as usize)];
		assert_eq!(deserialized, expected);
	}
}
