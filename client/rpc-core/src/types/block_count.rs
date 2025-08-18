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

use std::{fmt, str::FromStr};

use ethereum_types::U256;
use serde::{
	de::{Error, Visitor},
	Deserialize, Deserializer, Serialize, Serializer,
};

/// Represents An RPC Api block count param, which can take the form of a number, an hex string, or a 32-bytes array
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum BlockCount {
	/// U256
	U256(U256),
	/// Number
	Num(u64),
}

impl<'a> Deserialize<'a> for BlockCount {
	fn deserialize<D>(deserializer: D) -> Result<BlockCount, D::Error>
	where
		D: Deserializer<'a>,
	{
		deserializer.deserialize_any(BlockCountVisitor)
	}
}

impl Serialize for BlockCount {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			BlockCount::U256(ref x) => x.serialize(serializer),
			BlockCount::Num(ref x) => serializer.serialize_str(&format!("0x{x:x}")),
		}
	}
}

struct BlockCountVisitor;

impl From<BlockCount> for U256 {
	fn from(block_count: BlockCount) -> U256 {
		match block_count {
			BlockCount::Num(n) => U256::from(n),
			BlockCount::U256(n) => n,
		}
	}
}

impl From<BlockCount> for u64 {
	fn from(block_count: BlockCount) -> u64 {
		match block_count {
			BlockCount::Num(n) => n,
			BlockCount::U256(n) => n.as_u64(),
		}
	}
}

impl Visitor<'_> for BlockCountVisitor {
	type Value = BlockCount;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		write!(
			formatter,
			"an intenger, a (both 0x-prefixed or not) hex string or byte array containing between (0; 32] bytes"
		)
	}

	fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
	where
		E: Error,
	{
		let number = value.parse::<u64>();
		match number {
			Ok(n) => Ok(BlockCount::Num(n)),
			Err(_) => U256::from_str(value).map(BlockCount::U256).map_err(|_| {
				Error::custom("Invalid block count: non-decimal or missing 0x prefix".to_string())
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
		Ok(BlockCount::Num(value))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn match_block_number(block_count: BlockCount) -> Option<U256> {
		match block_count {
			BlockCount::Num(n) => Some(U256::from(n)),
			BlockCount::U256(n) => Some(n),
		}
	}

	#[test]
	fn block_number_deserialize() {
		let bn_dec: BlockCount = serde_json::from_str(r#""42""#).unwrap();
		let bn_hex: BlockCount = serde_json::from_str(r#""0x45""#).unwrap();
		assert_eq!(match_block_number(bn_dec).unwrap(), U256::from(42));
		assert_eq!(match_block_number(bn_hex).unwrap(), U256::from("0x45"));
	}
}
