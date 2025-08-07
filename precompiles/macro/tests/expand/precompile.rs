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

use core::marker::PhantomData;
use frame_support::pallet_prelude::{ConstU32, Get};
use precompile_utils::{prelude::*, EvmResult};
use sp_core::{H160, U256};

// Based on Batch with stripped code.

struct BatchPrecompile<Runtime>(PhantomData<Runtime>);

type GetCallDataLimit = ConstU32<42>;
type GetArrayLimit = ConstU32<42>;

#[precompile_utils_macro::precompile]
impl<Runtime> BatchPrecompile<Runtime>
where
	Runtime: Get<u32>,
{
	#[precompile::pre_check]
	fn pre_check(handle: &mut impl PrecompileHandle) -> EvmResult {
		todo!("pre_check")
	}

	#[precompile::public("batchSome(address[],uint256[],bytes[],uint64[])")]
	fn batch_some(
		handle: &mut impl PrecompileHandle,
		to: BoundedVec<Address, GetArrayLimit>,
		value: BoundedVec<U256, GetArrayLimit>,
		call_data: BoundedVec<BoundedBytes<GetCallDataLimit>, GetArrayLimit>,
		gas_limit: BoundedVec<u64, GetArrayLimit>,
	) -> EvmResult {
		todo!("batch_some")
	}

	#[precompile::public("batchSomeUntilFailure(address[],uint256[],bytes[],uint64[])")]
	fn batch_some_until_failure(
		handle: &mut impl PrecompileHandle,
		to: BoundedVec<Address, GetArrayLimit>,
		value: BoundedVec<U256, GetArrayLimit>,
		call_data: BoundedVec<BoundedBytes<GetCallDataLimit>, GetArrayLimit>,
		gas_limit: BoundedVec<u64, GetArrayLimit>,
	) -> EvmResult {
		todo!("batch_some_until_failure")
	}

	#[precompile::public("batchAll(address[],uint256[],bytes[],uint64[])")]
	fn batch_all(
		handle: &mut impl PrecompileHandle,
		to: BoundedVec<Address, GetArrayLimit>,
		value: BoundedVec<U256, GetArrayLimit>,
		call_data: BoundedVec<BoundedBytes<GetCallDataLimit>, GetArrayLimit>,
		gas_limit: BoundedVec<u64, GetArrayLimit>,
	) -> EvmResult {
		todo!("batch_all")
	}

	// additional function to check fallback
	#[precompile::fallback]
	fn fallback(handle: &mut impl PrecompileHandle) -> EvmResult {
		todo!("fallback")
	}
}
