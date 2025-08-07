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

use ethereum_types::{Address, U128, U256, U64};
use serde::{
	de,
	ser::{self, SerializeStruct},
	Deserialize, Serialize,
};

use crate::{access_list::AccessList, bytes::Bytes, transaction::TxType};

/// Transaction request from the RPC.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRequest {
	/// [EIP-2718](https://eips.ethereum.org/EIPS/eip-2718) transaction type
	#[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
	pub tx_type: Option<TxType>,

	/// Sender
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub from: Option<Address>,
	/// Recipient
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub to: Option<Address>,

	/// Value of transaction in wei
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub value: Option<U256>,
	/// Transaction's nonce
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub nonce: Option<U64>,

	/// Gas limit
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gas: Option<U128>,
	/// The gas price willing to be paid by the sender in wei
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U128>,
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee and miner / priority fee) in wei
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U128>,
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub max_priority_fee_per_gas: Option<U128>,

	/// Additional data
	#[serde(default, flatten)]
	pub input: TransactionInput,

	/// Chain ID that this transaction is valid on
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U64>,

	/// EIP-2930 access list
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub access_list: Option<AccessList>,
}

impl TransactionRequest {
	/// Sets the transactions type for the transactions.
	#[inline]
	pub const fn tx_type(mut self, tx_type: TxType) -> Self {
		self.tx_type = Some(tx_type);
		self
	}

	/// Sets the `from` field in the call to the provided address
	#[inline]
	pub const fn from(mut self, from: Address) -> Self {
		self.from = Some(from);
		self
	}

	/// Sets the recipient address for the transaction.
	#[inline]
	pub const fn to(mut self, to: Address) -> Self {
		self.to = Some(to);
		self
	}

	/// Sets the nonce for the transaction.
	#[inline]
	pub const fn nonce(mut self, nonce: U64) -> Self {
		self.nonce = Some(nonce);
		self
	}

	/// Sets the value (amount) for the transaction.
	#[inline]
	pub const fn value(mut self, value: U256) -> Self {
		self.value = Some(value);
		self
	}

	/// Sets the gas limit for the transaction.
	#[inline]
	pub const fn gas_limit(mut self, gas_limit: U128) -> Self {
		self.gas = Some(gas_limit);
		self
	}

	/// Sets the maximum fee per gas for the transaction.
	#[inline]
	pub const fn max_fee_per_gas(mut self, max_fee_per_gas: U128) -> Self {
		self.max_fee_per_gas = Some(max_fee_per_gas);
		self
	}

	/// Sets the maximum priority fee per gas for the transaction.
	#[inline]
	pub const fn max_priority_fee_per_gas(mut self, max_priority_fee_per_gas: U128) -> Self {
		self.max_priority_fee_per_gas = Some(max_priority_fee_per_gas);
		self
	}

	/// Sets the input data for the transaction.
	pub fn input(mut self, input: TransactionInput) -> Self {
		self.input = input;
		self
	}

	/// Sets the access list for the transaction.
	pub fn access_list(mut self, access_list: AccessList) -> Self {
		self.access_list = Some(access_list);
		self
	}

	/// Returns the configured fee cap, if any.
	///
	/// The returns `gas_price` (legacy) if set or `max_fee_per_gas` (EIP1559)
	#[inline]
	pub fn fee_cap(&self) -> Option<U128> {
		self.gas_price.or(self.max_fee_per_gas)
	}
}

/// Additional data of the transaction.
///
/// We accept (older) "data" and (newer) "input" for backwards-compatibility reasons.
/// If both fields are set, it is expected that they contain the same value, otherwise an error is returned.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TransactionInput {
	/// Transaction data
	pub input: Option<Bytes>,
	/// Transaction data
	///
	/// This is the same as `input` but is used for backwards compatibility: <https://github.com/ethereum/go-ethereum/issues/15628>
	pub data: Option<Bytes>,
}

impl TransactionInput {
	/// Return the additional data of the transaction.
	pub fn into_bytes(self) -> Option<Bytes> {
		match (self.input, self.data) {
			(Some(input), _) => Some(input),
			(None, Some(data)) => Some(data),
			(None, None) => None,
		}
	}
}

impl serde::Serialize for TransactionInput {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match (&self.input, &self.data) {
			(Some(input), Some(data)) => {
				if input == data {
					let mut s =
						serde::Serializer::serialize_struct(serializer, "TransactionInput", 2)?;
					s.serialize_field("input", input)?;
					s.serialize_field("data", data)?;
					s.end()
				} else {
					Err(ser::Error::custom("Ambiguous value for `input` and `data`"))
				}
			}
			(Some(input), None) => {
				let mut s = serde::Serializer::serialize_struct(serializer, "TransactionInput", 1)?;
				s.serialize_field("input", input)?;
				s.skip_field("data")?;
				s.end()
			}
			(None, Some(data)) => {
				let mut s = serde::Serializer::serialize_struct(serializer, "TransactionInput", 1)?;
				s.skip_field("input")?;
				s.serialize_field("data", data)?;
				s.end()
			}
			(None, None) => {
				let mut s = serde::Serializer::serialize_struct(serializer, "TransactionInput", 0)?;
				s.skip_field("input")?;
				s.skip_field("data")?;
				s.end()
			}
		}
	}
}

impl<'de> serde::Deserialize<'de> for TransactionInput {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		#[derive(Deserialize)]
		struct InputOrData {
			input: Option<Bytes>,
			data: Option<Bytes>,
		}

		let InputOrData { input, data } = InputOrData::deserialize(deserializer)?;

		match (input, data) {
			(Some(input), Some(data)) => {
				if input == data {
					Ok(Self {
						input: Some(input),
						data: Some(data),
					})
				} else {
					Err(de::Error::custom("Ambiguous value for `input` and `data`"))
				}
			}
			(input, data) => Ok(Self { input, data }),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn transaction_input_serde_impl() {
		let valid_cases = [
			(
				r#"{"input":"0x12","data":"0x12"}"#,
				TransactionInput {
					input: Some(Bytes(vec![0x12])),
					data: Some(Bytes(vec![0x12])),
				},
			),
			(
				r#"{"input":"0x12"}"#,
				TransactionInput {
					input: Some(Bytes(vec![0x12])),
					data: None,
				},
			),
			(
				r#"{"data":"0x12"}"#,
				TransactionInput {
					input: None,
					data: Some(Bytes(vec![0x12])),
				},
			),
			(
				r#"{}"#,
				TransactionInput {
					input: None,
					data: None,
				},
			),
		];
		for (raw, typed) in valid_cases {
			let deserialized = serde_json::from_str::<TransactionInput>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw);
		}

		let invalid_serialization_cases = [TransactionInput {
			input: Some(Bytes(vec![0x12])),
			data: Some(Bytes(vec![0x23])),
		}];
		for typed in invalid_serialization_cases {
			let serialized: Result<String, _> = serde_json::to_string(&typed);
			assert!(serialized.is_err());
		}

		let invalid_deserialization_cases = [r#"{"input":"0x12","data":"0x23"}"#];
		for raw in invalid_deserialization_cases {
			let input: Result<TransactionInput, _> = serde_json::from_str(raw);
			assert!(input.is_err());
		}
	}
}
