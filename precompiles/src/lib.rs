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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

// Allows to use inside this crate `solidity::Codec` derive macro,which depends on
// `precompile_utils` being in the list of imported crates.
extern crate self as precompile_utils;

#[doc(hidden)]
pub mod __alloc {
	pub use ::alloc::*;
}

pub mod evm;
pub mod precompile_set;
pub mod substrate;

pub mod solidity;

#[cfg(feature = "testing")]
pub mod testing;

pub use fp_evm::Precompile;
use fp_evm::PrecompileFailure;
pub use precompile_utils_macro::{keccak256, precompile, precompile_name_from_address};

/// Alias for Result returning an EVM precompile error.
pub type EvmResult<T = ()> = Result<T, PrecompileFailure>;

pub mod prelude {
	pub use {
		crate::{
			evm::{
				handle::PrecompileHandleExt,
				logs::{log0, log1, log2, log3, log4, LogExt},
			},
			precompile_set::DiscriminantResult,
			solidity::{
				// We export solidity itself to encourage using `solidity::Codec` to avoid confusion
				// with parity_scale_codec,
				self,
				codec::{
					Address,
					BoundedBytes,
					BoundedString,
					BoundedVec,
					// Allow usage of Codec methods while not exporting the name directly.
					Codec as _,
					Convert,
					UnboundedBytes,
					UnboundedString,
				},
				revert::{
					revert, BacktraceExt, InjectBacktrace, MayRevert, Revert, RevertExt,
					RevertReason,
				},
			},
			substrate::{RuntimeHelper, TryDispatchError},
			EvmResult,
		},
		alloc::string::String,
		pallet_evm::{PrecompileHandle, PrecompileOutput},
		precompile_utils_macro::{keccak256, precompile},
	};
}
