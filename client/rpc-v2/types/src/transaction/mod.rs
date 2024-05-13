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

mod receipt;
mod request;
mod signature;

use ethereum_types::{Address, H256, U256, U64};
use serde::{Deserialize, Serialize};

pub use self::{receipt::*, request::*, signature::*};
use crate::{access_list::AccessList, bytes::Bytes};

/// [EIP-2718](https://eips.ethereum.org/EIPS/eip-2718) transaction type.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Default)]
#[repr(u8)]
pub enum TxType {
	/// Legacy transaction
	#[default]
	Legacy = 0u8,
	/// [EIP-2930](https://eips.ethereum.org/EIPS/eip-2930) transaction
	EIP2930 = 1u8,
	/// [EIP-1559](https://eips.ethereum.org/EIPS/eip-1559) transaction
	EIP1559 = 2u8,
}

impl TryFrom<u8> for TxType {
	type Error = &'static str;

	fn try_from(value: u8) -> Result<Self, Self::Error> {
		match value {
			0u8 => Ok(Self::Legacy),
			1u8 => Ok(Self::EIP2930),
			2u8 => Ok(Self::EIP1559),
			_ => Err("Unsupported transaction type"),
		}
	}
}

impl serde::Serialize for TxType {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::Legacy => serializer.serialize_str("0x0"),
			Self::EIP2930 => serializer.serialize_str("0x1"),
			Self::EIP1559 => serializer.serialize_str("0x2"),
		}
	}
}

impl<'de> serde::Deserialize<'de> for TxType {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		match s.as_str() {
			"0x0" => Ok(Self::Legacy),
			"0x1" => Ok(Self::EIP2930),
			"0x2" => Ok(Self::EIP1559),
			_ => Err(serde::de::Error::custom("Unsupported transaction type")),
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
pub struct Transaction {
	/// [EIP-2718](https://eips.ethereum.org/EIPS/eip-27    gg  ) transaction type
	#[serde(rename = "type")]
	pub tx_type: TxType,

	/// Transaction hash
	pub hash: H256,
	/// Nonce
	pub nonce: U64,
	/// Block hash
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub block_hash: Option<H256>,
	/// Block number
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub block_number: Option<U256>,
	/// Transaction index
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub transaction_index: Option<U256>,
	/// Sender
	pub from: Address,
	/// Recipient
	pub to: Option<Address>,
	/// Transferred value
	pub value: U256,
	/// Input data
	pub input: Bytes,

	/// Gas limit
	pub gas: U64,
	/// Gas price
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U256>,
	/// Max BaseFeePerGas the user is willing to pay
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	/// The miner's tip
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub max_priority_fee_per_gas: Option<U256>,

	/// Chain ID that this transaction is valid on
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U64>,
	/// All _flattened_ fields of the transaction signature
	#[serde(flatten)]
	pub signature: TransactionSignature,

	/// EIP-2930 access list
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub access_list: Option<AccessList>,
}
