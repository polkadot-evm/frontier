// This file is part of Tokfin.

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

use ethereum_types::U256;
use serde::{Deserialize, Serialize};

/// Transaction signature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionSignature {
	/// The R field of the signature
	pub r: U256,
	/// The S field of the signature
	pub s: U256,

	/// The standardised V field of the signature.
	///
	/// - For legacy transactions, this is the recovery id.
	/// - For typed transactions (EIP-2930, EIP-1559, EIP-4844), this is set to the parity
	///   (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	///
	/// # Note
	///
	/// For backwards compatibility, `v` is optionally provided as an alternative to `yParity`.
	/// This field is DEPRECATED and all use of it should migrate to `yParity`.
	pub v: U256,
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	///
	/// This is only used for typed (non-legacy) transactions.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub y_parity: Option<Parity>,
}

/// Type that represents the parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
///
/// This will be serialized as "0x0" if false, and "0x1" if true.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Parity(pub bool);

impl serde::Serialize for Parity {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		serializer.serialize_str(if self.0 { "0x1" } else { "0x0" })
	}
}

impl<'de> serde::Deserialize<'de> for Parity {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		match s.as_str() {
			"0x0" => Ok(Self(false)),
			"0x1" => Ok(Self(true)),
			_ => Err(serde::de::Error::custom(format!(
				"invalid parity value, parity should be either \"0x0\" or \"0x1\": {s}",
			))),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parity_serde_impl() {
		let valid_cases = [(r#""0x1""#, Parity(true)), (r#""0x0""#, Parity(false))];
		for (raw, typed) in valid_cases {
			let deserialized = serde_json::from_str::<Parity>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw);
		}

		let invalid_cases = [r#""0x2""#, r#""0x""#, r#""0""#, r#""1""#];
		for raw in invalid_cases {
			let parity: Result<Parity, _> = serde_json::from_str(raw);
			assert!(parity.is_err());
		}
	}

	#[test]
	fn signature_serde_impl() {
		let cases = [
			// without parity
			(
				r#"{
					"r":"0xab3743210536a011365f73bc6e25668177203562aa53741086f56d1ef3e101c0",
					"s":"0x479de4b30541dd1d3b73d5b9d8393d48d91d64ca3ff71f64bd7adaac2657a8e5",
					"v":"0x1546d71"
				}"#,
				TransactionSignature {
					r: "0xab3743210536a011365f73bc6e25668177203562aa53741086f56d1ef3e101c0"
						.parse()
						.unwrap(),
					s: "0x479de4b30541dd1d3b73d5b9d8393d48d91d64ca3ff71f64bd7adaac2657a8e5"
						.parse()
						.unwrap(),
					v: "0x1546d71".parse().unwrap(),
					y_parity: None,
				},
			),
			// with parity
			(
				r#"{
					"r":"0x39614515ff2794c0e005b33dd05e2cdce7857ae7ee47e9b6aa739c314c760f5",
					"s":"0x32670b1a7dbf2700e5fb65eb8e24c87ba18694a11fae98e5cf731f10f27f1f72",
					"v":"0x1",
					"yParity":"0x1"
				}"#,
				TransactionSignature {
					r: "0x39614515ff2794c0e005b33dd05e2cdce7857ae7ee47e9b6aa739c314c760f5"
						.parse()
						.unwrap(),
					s: "0x32670b1a7dbf2700e5fb65eb8e24c87ba18694a11fae98e5cf731f10f27f1f72"
						.parse()
						.unwrap(),
					v: "0x1".parse().unwrap(),
					y_parity: Some(Parity(true)),
				},
			),
		];

		for (raw, typed) in cases {
			let deserialized = serde_json::from_str::<TransactionSignature>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw.split_whitespace().collect::<String>());
		}
	}
}
