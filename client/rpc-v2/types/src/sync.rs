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

use ethereum_types::U64;
use serde::{de, Deserialize, Serialize};

/// The syncing status of client.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SyncingStatus {
	/// Progress when syncing.
	IsSyncing(SyncingProgress),
	/// Not syncing.
	NotSyncing,
}

impl serde::Serialize for SyncingStatus {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::IsSyncing(progress) => progress.serialize(serializer),
			Self::NotSyncing => serializer.serialize_bool(false),
		}
	}
}

impl<'de> serde::Deserialize<'de> for SyncingStatus {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		#[derive(Deserialize)]
		#[serde(untagged)]
		enum Syncing {
			IsSyncing(SyncingProgress),
			NotSyncing(bool),
		}

		match Syncing::deserialize(deserializer)? {
			Syncing::IsSyncing(sync) => Ok(Self::IsSyncing(sync)),
			Syncing::NotSyncing(false) => Ok(Self::NotSyncing),
			Syncing::NotSyncing(true) => Err(de::Error::custom(
				"eth_syncing should always return false if not syncing.",
			)),
		}
	}
}

/// The syncing progress.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncingProgress {
	/// Block number this node started to synchronize from.
	pub starting_block: U64,
	/// Block number this node is currently importing.
	pub current_block: U64,
	/// Block number of the highest block header this node has received from peers.
	pub highest_block: U64,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn syncing_status_serde_impl() {
		let valid_cases = [
			(
				r#"{"startingBlock":"0x64","currentBlock":"0xc8","highestBlock":"0x12c"}"#,
				SyncingStatus::IsSyncing(SyncingProgress {
					starting_block: 100.into(),
					current_block: 200.into(),
					highest_block: 300.into(),
				}),
			),
			("false", SyncingStatus::NotSyncing),
		];
		for (raw, typed) in valid_cases {
			let deserialized = serde_json::from_str::<SyncingStatus>(raw).unwrap();
			assert_eq!(deserialized, typed);

			let serialized = serde_json::to_string(&typed).unwrap();
			assert_eq!(serialized, raw);
		}

		let invalid_cases = ["true"];
		for raw in invalid_cases {
			let status: Result<SyncingStatus, _> = serde_json::from_str(raw);
			assert!(status.is_err());
		}
	}
}
