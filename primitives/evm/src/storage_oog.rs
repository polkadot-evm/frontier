// This file is part of Frontier.

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

environmental::environmental!(STORAGE_OOG: bool);

use crate::{ExitError, ExitReason};
use sp_core::U256;

pub fn handle_storage_oog<R, F>(gas_limit: u64, f: F) -> (ExitReason, R, u64, U256)
where
	F: FnOnce() -> (ExitReason, R, u64, U256),
	R: Default,
{
	STORAGE_OOG::using_once(&mut false, || {
		let (reason, retv, used_gas, effective_gas) = f();

		STORAGE_OOG::with(|storage_oog| {
			if *storage_oog {
				(
					ExitReason::Error(ExitError::OutOfGas),
					Default::default(),
					used_gas,
					U256([gas_limit, 0, 0, 0]),
				)
			} else {
				(reason, retv, used_gas, effective_gas)
			}
		})
		// This should always return `Some`, but let's play it safe.
		.expect("STORAGE_OOG not defined")
	})
}

pub(super) fn set_storage_oog() {
	STORAGE_OOG::with(|storage_oog| {
		*storage_oog = true;
	});
}
