// This file is part of Tokfin.

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

use crate::{
	solidity::codec::Writer,
	testing::{decode_revert_message, MockHandle},
};
use fp_evm::{Context, PrecompileFailure, PrecompileSet};
use sp_core::{H160, U256};

pub struct PrecompilesModifierTester<P> {
	precompiles: P,
	handle: MockHandle,
}

impl<P: PrecompileSet> PrecompilesModifierTester<P> {
	pub fn new(precompiles: P, from: impl Into<H160>, to: impl Into<H160>) -> Self {
		let to = to.into();
		let mut handle = MockHandle::new(
			to,
			Context {
				address: to,
				caller: from.into(),
				apparent_value: U256::zero(),
			},
		);

		handle.gas_limit = u64::MAX;

		Self {
			precompiles,
			handle,
		}
	}

	fn is_view(&mut self, selector: u32) -> bool {
		// View: calling with static should not revert with static-related message.
		let handle = &mut self.handle;
		handle.is_static = true;
		handle.context.apparent_value = U256::zero();
		handle.input = Writer::new_with_selector(selector).build();

		let res = self.precompiles.execute(handle);

		match res {
			Some(Err(PrecompileFailure::Revert { output, .. })) => {
				let decoded = decode_revert_message(&output);

				dbg!(decoded) != b"Can't call non-static function in static context"
			}
			Some(_) => true,
			None => panic!("tried to check view modifier on unknown precompile"),
		}
	}

	fn is_payable(&mut self, selector: u32) -> bool {
		// Payable: calling with value should not revert with payable-related message.
		let handle = &mut self.handle;
		handle.is_static = false;
		handle.context.apparent_value = U256::one();
		handle.input = Writer::new_with_selector(selector).build();

		let res = self.precompiles.execute(handle);

		match res {
			Some(Err(PrecompileFailure::Revert { output, .. })) => {
				let decoded = decode_revert_message(&output);

				decoded != b"Function is not payable"
			}
			Some(_) => true,
			None => panic!("tried to check payable modifier on unknown precompile"),
		}
	}

	pub fn test_view_modifier(&mut self, selectors: &[u32]) {
		for &s in selectors {
			assert!(
				self.is_view(s),
				"Function doesn't behave like a view function."
			);
			assert!(
				!self.is_payable(s),
				"Function doesn't behave like a non-payable function."
			)
		}
	}

	pub fn test_payable_modifier(&mut self, selectors: &[u32]) {
		for &s in selectors {
			assert!(
				!self.is_view(s),
				"Function doesn't behave like a non-view function."
			);
			assert!(
				self.is_payable(s),
				"Function doesn't behave like a payable function."
			);
		}
	}

	pub fn test_default_modifier(&mut self, selectors: &[u32]) {
		for &s in selectors {
			assert!(
				!self.is_view(s),
				"Function doesn't behave like a non-view function."
			);
			assert!(
				!self.is_payable(s),
				"Function doesn't behave like a non-payable function."
			);
		}
	}
}
