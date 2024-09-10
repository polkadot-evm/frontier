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

use std::{fmt, ops};

#[derive(Clone, Default, Eq, PartialEq, Hash)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
	pub fn new(bytes: Vec<u8>) -> Self {
		Self(bytes)
	}

	pub fn into_vec(self) -> Vec<u8> {
		self.0
	}
}

impl fmt::Debug for Bytes {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::LowerHex::fmt(self, f)
	}
}

impl fmt::Display for Bytes {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::LowerHex::fmt(self, f)
	}
}

impl fmt::LowerHex for Bytes {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.pad(&const_hex::encode_prefixed(self.as_ref()))
	}
}

impl fmt::UpperHex for Bytes {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.pad(&const_hex::encode_upper_prefixed(self.as_ref()))
	}
}

impl ops::Deref for Bytes {
	type Target = [u8];

	#[inline]
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl ops::DerefMut for Bytes {
	#[inline]
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl AsRef<[u8]> for Bytes {
	#[inline]
	fn as_ref(&self) -> &[u8] {
		self.0.as_ref()
	}
}

impl From<Vec<u8>> for Bytes {
	fn from(bytes: Vec<u8>) -> Bytes {
		Bytes(bytes)
	}
}

impl From<Bytes> for Vec<u8> {
	fn from(bytes: Bytes) -> Vec<u8> {
		bytes.0
	}
}

impl serde::Serialize for Bytes {
	#[inline]
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		if serializer.is_human_readable() {
			const_hex::serialize(self, serializer)
		} else {
			serializer.serialize_bytes(self.as_ref())
		}
	}
}

impl<'de> serde::Deserialize<'de> for Bytes {
	#[inline]
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		use serde::de;

		struct BytesVisitor;

		impl<'de> de::Visitor<'de> for BytesVisitor {
			type Value = Bytes;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str(
					"a variable number of bytes represented as a hex string, an array of u8, or raw bytes",
				)
			}

			fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				if v.is_empty() {
					return Err(de::Error::invalid_value(
						de::Unexpected::Str(v),
						&"a valid hex string",
					));
				}

				const_hex::decode(v)
					.map_err(|_| {
						de::Error::invalid_value(de::Unexpected::Str(v), &"a valid hex string")
					})
					.map(From::from)
			}

			fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				self.visit_str(value.as_ref())
			}

			fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
				Ok(Bytes::from(v.to_vec()))
			}

			fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
				Ok(Bytes::from(v))
			}

			fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
				let mut bytes = Vec::with_capacity(seq.size_hint().unwrap_or(0));

				while let Some(byte) = seq.next_element()? {
					bytes.push(byte);
				}

				Ok(Bytes::from(bytes))
			}
		}

		if deserializer.is_human_readable() {
			deserializer.deserialize_any(BytesVisitor)
		} else {
			deserializer.deserialize_byte_buf(BytesVisitor)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bytes_serialize() {
		let bytes = const_hex::decode("0123456789abcdef").unwrap();
		let bytes = Bytes::new(bytes);
		let serialized = serde_json::to_string(&bytes).unwrap();
		assert_eq!(serialized, r#""0x0123456789abcdef""#);
	}

	#[test]
	fn bytes_deserialize() {
		let bytes0: Result<Bytes, serde_json::Error> = serde_json::from_str(r#""∀∂""#);
		let bytes1: Result<Bytes, serde_json::Error> = serde_json::from_str(r#""""#);
		let bytes2: Result<Bytes, serde_json::Error> = serde_json::from_str(r#""0x123""#);
		let bytes3: Result<Bytes, serde_json::Error> = serde_json::from_str(r#""0xgg""#);

		let bytes4: Bytes = serde_json::from_str(r#""0x""#).unwrap();
		let bytes5: Bytes = serde_json::from_str(r#""0x12""#).unwrap();
		let bytes6: Bytes = serde_json::from_str(r#""0x0123""#).unwrap();

		assert!(bytes0.is_err());
		assert!(bytes1.is_err());
		assert!(bytes2.is_err());
		assert!(bytes3.is_err());
		assert_eq!(bytes4, Bytes(vec![]));
		assert_eq!(bytes5, Bytes(vec![0x12]));
		assert_eq!(bytes6, Bytes(vec![0x1, 0x23]));
	}
}
