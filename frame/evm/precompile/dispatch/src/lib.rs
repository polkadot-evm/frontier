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

extern crate alloc;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use alloc::format;
use core::marker::PhantomData;
use fp_evm::{
	ExitError, ExitSucceed, Precompile, PrecompileFailure, PrecompileHandle, PrecompileOutput,
	PrecompileResult,
};
use frame_support::{
	codec::{Decode, DecodeLimit as _},
	dispatch::{DispatchClass, Dispatchable, GetDispatchInfo, Pays, PostDispatchInfo},
	traits::{ConstU32, Get},
};
use pallet_evm::{AddressMapping, GasWeightMapping};

// `DecodeLimit` specifies the max depth a call can use when decoding, as unbounded depth
// can be used to overflow the stack.
// Default value is 8, which is the same as in XCM call decoding.
pub struct Dispatch<T, DecodeLimit = ConstU32<8>> {
	_marker: PhantomData<(T, DecodeLimit)>,
}

impl<T, DecodeLimit> Precompile for Dispatch<T, DecodeLimit>
where
	T: pallet_evm::Config,
	T::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo + Decode,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<T::AccountId>>,
	DecodeLimit: Get<u32>,
{
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		let input = handle.input();
		let target_gas = handle.gas_limit();
		let context = handle.context();

		let call = T::RuntimeCall::decode_with_depth_limit(DecodeLimit::get(), &mut &*input)
			.map_err(|_| PrecompileFailure::Error {
				exit_status: ExitError::Other("decode failed".into()),
			})?;
		let info = call.get_dispatch_info();

		let valid_call = info.pays_fee == Pays::Yes && info.class == DispatchClass::Normal;
		if !valid_call {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid call".into()),
			});
		}

		if let Some(gas) = target_gas {
			let valid_weight =
				info.weight.ref_time() <= T::GasWeightMapping::gas_to_weight(gas, false).ref_time();
			if !valid_weight {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::OutOfGas,
				});
			}
		}

		let origin = T::AddressMapping::into_account_id(context.caller);

		match call.dispatch(Some(origin).into()) {
			Ok(post_info) => {
				let cost = T::GasWeightMapping::weight_to_gas(
					post_info.actual_weight.unwrap_or(info.weight),
				);

				handle.record_cost(cost)?;

				Ok(PrecompileOutput {
					exit_status: ExitSucceed::Stopped,
					output: Default::default(),
				})
			}
			Err(e) => Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(
					format!("dispatch execution failed: {}", <&'static str>::from(e)).into(),
				),
			}),
		}
	}
}
