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

//! Provide checks related to function modifiers (view/payable).

use crate::solidity::revert::{MayRevert, RevertReason};
use fp_evm::Context;
use sp_core::U256;

/// Represents modifiers a Solidity function can be annotated with.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum FunctionModifier {
	/// Function that doesn't modify the state.
	View,
	/// Function that modifies the state but refuse receiving funds.
	/// Correspond to a Solidity function with no modifiers.
	NonPayable,
	/// Function that modifies the state and accept funds.
	Payable,
}

/// Check that a function call is compatible with the context it is
/// called into.
pub fn check_function_modifier(
	context: &Context,
	is_static: bool,
	modifier: FunctionModifier,
) -> MayRevert {
	if is_static && modifier != FunctionModifier::View {
		return Err(
			RevertReason::custom("Can't call non-static function in static context").into(),
		);
	}

	if modifier != FunctionModifier::Payable && context.apparent_value > U256::zero() {
		return Err(RevertReason::custom("Function is not payable").into());
	}

	Ok(())
}
