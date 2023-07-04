// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2022 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use ethereum::AccessListItem;
use ethereum_types::{H160, H256, U256};
use serde::{de::{Visitor, MapAccess, self}, Deserialize, Deserializer};

use crate::types::Bytes;

/// Call request
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallRequest {
	/// From
	pub from: Option<H160>,
	/// To
	pub to: Option<H160>,
	/// Gas Price
	pub gas_price: Option<U256>,
	/// EIP-1559 Max base fee the caller is willing to pay
	pub max_fee_per_gas: Option<U256>,
	/// EIP-1559 Priority fee the caller is paying to the block author
	pub max_priority_fee_per_gas: Option<U256>,
	/// Gas
	pub gas: Option<U256>,
	/// Value
	pub value: Option<U256>,
	/// Data
	pub data: Option<Bytes>,
	/// Input
	pub input: Option<Bytes>,
	/// Nonce
	pub nonce: Option<U256>,
	/// AccessList
	pub access_list: Option<Vec<AccessListItem>>,
	/// EIP-2718 type
	pub transaction_type: Option<U256>,
}

// State override
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CallStateOverride {
	/// Fake balance to set for the account before executing the call.
	pub balance: Option<U256>,
	/// Fake nonce to set for the account before executing the call.
	pub nonce: Option<U256>,
	/// Fake EVM bytecode to inject into the account before executing the call.
	pub code: Option<Bytes>,
	/// Fake key-value mapping to override all slots in the account storage before
	/// executing the call.
	pub state: Option<BTreeMap<H256, H256>>,
	/// Fake key-value mapping to override individual slots in the account storage before
	/// executing the call.
	pub state_diff: Option<BTreeMap<H256, H256>>,
}

impl<'de> Deserialize<'de> for CallRequest {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		#[derive(Deserialize)]
		#[serde(field_identifier, rename_all = "camelCase")]
		enum Field {
			From,
			To,
			GasPrice,
			MaxFeePerGas,
			MaxPriorityFeePerGas,
			Gas,
			Value,
			Data,
			Input,
			Nonce,
			AccessList,
			Type,
		}

		struct CallRequestVisitor;

		impl<'de> Visitor<'de> for CallRequestVisitor {
			type Value = CallRequest;

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				formatter.write_str("struct CallRequest")
			}

			fn visit_map<V>(self, mut map: V) -> Result<CallRequest, V::Error>
			where
				V: MapAccess<'de>,
			{
				let mut from = None;
				let mut to = None;
				let mut gas_price = None;
				let mut max_fee_per_gas = None;
				let mut max_priority_fee_per_gas = None;
				let mut gas = None;
				let mut value = None;
				let mut data = None;
				let mut input = None;
				let mut nonce = None;
				let mut access_list = None;
				let mut transaction_type = None;

				while let Some(key) = map.next_key()? {
					match key {
						Field::From => {
							if from.is_some() {
								return Err(de::Error::duplicate_field("from"));
							}
							from = Some(map.next_value()?);
						}
						Field::To => {
							if to.is_some() {
								return Err(de::Error::duplicate_field("to"));
							}
							to = Some(map.next_value()?);
						}
						Field::GasPrice => {
							if gas_price.is_some() {
								return Err(de::Error::duplicate_field("gasPrice"));
							}
							gas_price = Some(map.next_value()?);
						}
						Field::MaxFeePerGas => {
							if max_fee_per_gas.is_some() {
								return Err(de::Error::duplicate_field("maxFeePerGas"));
							}
							max_fee_per_gas = Some(map.next_value()?);
						}
						Field::MaxPriorityFeePerGas => {
							if max_priority_fee_per_gas.is_some() {
								return Err(de::Error::duplicate_field("maxPriorityFeePerGas"));
							}
							max_priority_fee_per_gas = Some(map.next_value()?);
						}
						Field::Gas => {
							if gas.is_some() {
								return Err(de::Error::duplicate_field("gas"));
							}
							gas = Some(map.next_value()?);
						}
						Field::Value => {
							if value.is_some() {
								return Err(de::Error::duplicate_field("value"));
							}
							value = Some(map.next_value()?);
						}
						Field::Data => {
							if data.is_some() {
								return Err(de::Error::duplicate_field("data"));
							}
							data = Some(map.next_value()?);
						}
						Field::Input => {
							if input.is_some() {
								return Err(de::Error::duplicate_field("input"));
							}
							input = Some(map.next_value()?);
						}
						Field::Nonce => {
							if nonce.is_some() {
								return Err(de::Error::duplicate_field("nonce"));
							}
							nonce = Some(map.next_value()?);
						}
						Field::AccessList => {
							if access_list.is_some() {
								return Err(de::Error::duplicate_field("accessList"));
							}
							access_list = Some(map.next_value()?);
						}
						Field::Type => {
							if transaction_type.is_some() {
								return Err(de::Error::duplicate_field("type"));
							}
							transaction_type = Some(map.next_value()?);
						}
					}
				}

				match (data.as_ref(), input.as_ref()) {
					(Some(data), Some(input)) if data != input => {
						return Err(de::Error::custom("data and input must be equal when both are present"))
					}
					(None, Some(_)) => data = input.take(),
					_ => {}
				}

				Ok(CallRequest {
					from,
					to,
					gas_price,
					max_fee_per_gas,
					max_priority_fee_per_gas,
					gas,
					value,
					data,
					input,
					nonce,
					access_list,
					transaction_type,
				})
			}
		}

		deserializer.deserialize_struct(
			"CallRequest",
			&[
				"from",
				"to",
				"gasPrice",
				"maxFeePerGas",
				"maxPriorityFeePerGas",
				"gas",
				"value",
				"data",
				"input",
				"nonce",
				"accessList",
				"type",
			],
			CallRequestVisitor,
		)
	}
}
