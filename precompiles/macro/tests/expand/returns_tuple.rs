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

use precompile_utils::{prelude::*, EvmResult};
use sp_core::{H160, U256};

struct ExamplePrecompile;

#[precompile_utils_macro::precompile]
impl ExamplePrecompile {
	#[precompile::public("example()")]
	fn example(handle: &mut impl PrecompileHandle) -> EvmResult<(Address, U256, UnboundedBytes)> {
		todo!("example")
	}
}
