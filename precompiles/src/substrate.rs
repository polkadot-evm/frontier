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

//! Utils related to Substrate features:
//! - Substrate call dispatch.
//! - Substrate DB read and write costs

use core::marker::PhantomData;

// Substrate
use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo},
	traits::Get,
	weights::Weight,
};
use sp_runtime::{traits::Dispatchable, DispatchError};
// Frontier
use fp_evm::{ExitError, PrecompileFailure, PrecompileHandle};
use pallet_evm::GasWeightMapping;

use crate::{evm::handle::using_precompile_handle, solidity::revert::revert};

#[derive(Debug)]
pub enum TryDispatchError {
	Evm(ExitError),
	Substrate(DispatchError),
}

impl From<TryDispatchError> for PrecompileFailure {
	fn from(f: TryDispatchError) -> PrecompileFailure {
		match f {
			TryDispatchError::Evm(e) => PrecompileFailure::Error { exit_status: e },
			TryDispatchError::Substrate(e) => {
				revert(alloc::format!("Dispatched call failed with error: {e:?}"))
			}
		}
	}
}

/// Helper functions requiring a Substrate runtime.
/// This runtime must of course implement `pallet_evm::Config`.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeHelper<Runtime>(PhantomData<Runtime>);

impl<Runtime> RuntimeHelper<Runtime>
where
	Runtime: pallet_evm::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
{
	#[inline(always)]
	pub fn record_external_cost(
		handle: &mut impl PrecompileHandle,
		weight: Weight,
		storage_growth: u64,
	) -> Result<(), ExitError> {
		// Make sure there is enough gas.
		let remaining_gas = handle.remaining_gas();
		let required_gas = Runtime::GasWeightMapping::weight_to_gas(weight);
		if required_gas > remaining_gas {
			return Err(ExitError::OutOfGas);
		}

		// Make sure there is enough remaining weight
		// TODO: record ref time when precompile will be benchmarked
		handle.record_external_cost(None, Some(weight.proof_size()), Some(storage_growth))
	}

	#[inline(always)]
	pub fn refund_weight_v2_cost(
		handle: &mut impl PrecompileHandle,
		weight: Weight,
		maybe_actual_weight: Option<Weight>,
	) -> Result<u64, ExitError> {
		// Refund weights and compute used weight them record used gas
		// TODO: refund ref time when precompile will be benchmarked
		let used_weight = if let Some(actual_weight) = maybe_actual_weight {
			let refund_weight = weight.checked_sub(&actual_weight).unwrap_or_default();
			handle.refund_external_cost(None, Some(refund_weight.proof_size()));
			actual_weight
		} else {
			weight
		};
		let used_gas = Runtime::GasWeightMapping::weight_to_gas(used_weight);
		handle.record_cost(used_gas)?;
		Ok(used_gas)
	}

	/// Try to dispatch a Substrate call.
	/// Return an error if there are not enough gas, or if the call fails.
	/// If successful returns the used gas using the Runtime GasWeightMapping.
	pub fn try_dispatch<Call>(
		handle: &mut impl PrecompileHandle,
		origin: <Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin,
		call: Call,
		storage_growth: u64,
	) -> Result<PostDispatchInfo, TryDispatchError>
	where
		Runtime::RuntimeCall: From<Call>,
	{
		let call = Runtime::RuntimeCall::from(call);
		let dispatch_info = call.get_dispatch_info();

		Self::record_external_cost(handle, dispatch_info.weight, storage_growth)
			.map_err(TryDispatchError::Evm)?;

		// Dispatch call.
		// It may be possible to not record gas cost if the call returns Pays::No.
		// However while Substrate handle checking weight while not making the sender pay for it,
		// the EVM doesn't. It seems this safer to always record the costs to avoid unmetered
		// computations.
		let post_dispatch_info = using_precompile_handle(handle, || call.dispatch(origin))
			.map_err(|e| TryDispatchError::Substrate(e.error))?;

		Self::refund_weight_v2_cost(
			handle,
			dispatch_info.weight,
			post_dispatch_info.actual_weight,
		)
		.map_err(TryDispatchError::Evm)?;

		Ok(post_dispatch_info)
	}
}

impl<Runtime> RuntimeHelper<Runtime>
where
	Runtime: pallet_evm::Config,
{
	/// Cost of a Substrate DB write in gas.
	pub fn db_write_gas_cost() -> u64 {
		<Runtime as pallet_evm::Config>::GasWeightMapping::weight_to_gas(
			<Runtime as frame_system::Config>::DbWeight::get().writes(1),
		)
	}

	/// Cost of a Substrate DB read in gas.
	pub fn db_read_gas_cost() -> u64 {
		<Runtime as pallet_evm::Config>::GasWeightMapping::weight_to_gas(
			<Runtime as frame_system::Config>::DbWeight::get().reads(1),
		)
	}
}
