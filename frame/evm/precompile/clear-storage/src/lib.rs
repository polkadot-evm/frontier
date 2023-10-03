// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
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

#![cfg_attr(not(feature = "std"), no_std)]
//#![deny(unused_crate_dependencies)]

extern crate alloc;

pub const ARRAY_LIMIT: u32 = 1_000;

use core::marker::PhantomData;
use precompile_utils::{prelude::*, EvmResult};
use sp_runtime::traits::ConstU32;

type GetArrayLimit = ConstU32<ARRAY_LIMIT>;

/// Storage cleanner precompile.
#[derive(Debug, Clone)]
pub struct StorageCleanerPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> StorageCleanerPrecompile<Runtime>
where
	Runtime: pallet_evm::Config,
{
	#[precompile::public("batchSome(address[])")]
	fn clear_suicided_storage(
		handle: &mut impl PrecompileHandle,
		contracts: BoundedVec<Address, GetArrayLimit>,
	) -> EvmResult {
		todo!()
	}
}
