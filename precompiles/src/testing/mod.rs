// This file is part of Frontier.

// Copyright (c) Moonsong Labs.
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

pub mod account;
pub mod execution;
pub mod handle;
pub mod modifier;
mod solidity;

pub use account::*;
pub use execution::*;
pub use handle::*;
pub use modifier::*;
pub use solidity::{check_precompile_implements_solidity_interfaces, compute_selector};

use fp_evm::Log;

pub fn decode_revert_message(encoded: &[u8]) -> &[u8] {
	let encoded_len = encoded.len();
	// selector 4 + offset 32 + string length 32
	if encoded_len > 68 {
		let message_len = encoded[36..68].iter().sum::<u8>();
		if encoded_len >= 68 + message_len as usize {
			return &encoded[68..68 + message_len as usize];
		}
	}
	b"decode_revert_message: error"
}

#[derive(Clone, PartialEq, Eq)]
pub struct PrettyLog(Log);

impl core::fmt::Debug for PrettyLog {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		let bytes = self
			.0
			.data
			.iter()
			.map(|b| format!("{b:02X}"))
			.collect::<Vec<String>>()
			.join("");

		let message = String::from_utf8(self.0.data.clone()).ok();

		f.debug_struct("Log")
			.field("address", &self.0.address)
			.field("topics", &self.0.topics)
			.field("data", &bytes)
			.field("data_utf8", &message)
			.finish()
	}
}

/// Panics if an event is not found in the system log of events
#[macro_export]
macro_rules! assert_event_emitted {
	($event:expr) => {
		match &$event {
			e => {
				assert!(
					$crate::mock::events().iter().find(|x| *x == e).is_some(),
					"Event {:?} was not found in events: \n {:?}",
					e,
					$crate::mock::events()
				);
			}
		}
	};
}

// Panics if an event is found in the system log of events
#[macro_export]
macro_rules! assert_event_not_emitted {
	($event:expr) => {
		match &$event {
			e => {
				assert!(
					$crate::mock::events().iter().find(|x| *x == e).is_none(),
					"Event {:?} was found in events: \n {:?}",
					e,
					$crate::mock::events()
				);
			}
		}
	};
}
