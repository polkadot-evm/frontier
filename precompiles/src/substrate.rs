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

/// Re-export Substrate's transactional storage helpers.
///
/// This is used by the `#[precompile]` macro to ensure `#[precompile::view]` calls
/// do not leave persistent storage side effects while staying compatible with
/// non-STATICCALL callers.
///
/// See: https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/support/src/storage/transactional.rs#L106
pub use frame_support::storage::transactional;
pub use sp_runtime::TransactionOutcome;

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

		Self::record_external_cost(handle, dispatch_info.total_weight(), storage_growth)
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
			dispatch_info.total_weight(),
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

#[cfg(all(test, feature = "testing"))]
mod transactional_view_tests {
	use crate::substrate::transactional;
	use crate::{prelude::*, testing::PrecompileTesterExt, EvmResult};
	use fp_evm::{Precompile, PrecompileFailure};
	use sp_core::H160;
	use sp_runtime::codec::Encode;

	const STORAGE_KEY: &[u8] = b"view-rollback-test-key";

	// ========================================================================
	// Test 1: Simple Precompile with a view function that modifies storage
	// ========================================================================

	pub struct SimplePrecompile;

	#[precompile_utils_macro::precompile]
	impl SimplePrecompile {
		#[precompile::public("viewThatWritesStorage()")]
		#[precompile::view]
		fn view_that_writes_storage(_handle: &mut impl PrecompileHandle) -> EvmResult {
			// This write should be rolled back because the function is marked as view
			sp_io::storage::set(STORAGE_KEY, b"modified-by-view");
			Ok(())
		}

		#[precompile::public("nonViewThatWritesStorage()")]
		fn non_view_that_writes_storage(_handle: &mut impl PrecompileHandle) -> EvmResult {
			// This write should persist because the function is NOT marked as view
			sp_io::storage::set(STORAGE_KEY, b"modified-by-non-view");
			Ok(())
		}
	}

	#[test]
	fn simple_precompile_view_rolls_back_storage() {
		let mut ext = sp_io::TestExternalities::default();
		ext.execute_with(|| {
			sp_io::storage::set(STORAGE_KEY, b"original");

			// Call the view function - storage changes should be rolled back
			let mut handle = crate::testing::MockHandle::new(
				H160::zero(),
				fp_evm::Context {
					address: H160::zero(),
					caller: H160::zero(),
					apparent_value: sp_core::U256::zero(),
				},
			);
			handle.input = SimplePrecompileCall::view_that_writes_storage {}.encode();

			let result = SimplePrecompile::execute(&mut handle);

			assert!(result.is_ok());
			assert_eq!(
				sp_io::storage::get(STORAGE_KEY).map(|b| b.to_vec()),
				Some(b"original".to_vec()),
				"View function should have rolled back the storage write"
			);
		});
	}

	#[test]
	fn simple_precompile_non_view_persists_storage() {
		let mut ext = sp_io::TestExternalities::default();
		ext.execute_with(|| {
			sp_io::storage::set(STORAGE_KEY, b"original");

			// Call the non-view function - storage changes should persist
			let mut handle = crate::testing::MockHandle::new(
				H160::zero(),
				fp_evm::Context {
					address: H160::zero(),
					caller: H160::zero(),
					apparent_value: sp_core::U256::zero(),
				},
			);
			handle.input = SimplePrecompileCall::non_view_that_writes_storage {}.encode();

			let result = SimplePrecompile::execute(&mut handle);

			assert!(result.is_ok());
			assert_eq!(
				sp_io::storage::get(STORAGE_KEY).map(|b| b.to_vec()),
				Some(b"modified-by-non-view".to_vec()),
				"Non-view function should have persisted the storage write"
			);
		});
	}

	#[test]
	fn simple_precompile_view_reverts_when_transactional_limit_is_reached() {
		let mut ext = sp_io::TestExternalities::default();
		ext.execute_with(|| {
			sp_io::storage::set(
				transactional::TRANSACTION_LEVEL_KEY,
				&transactional::TRANSACTIONAL_LIMIT.encode(),
			);

			let mut handle = crate::testing::MockHandle::new(
				H160::zero(),
				fp_evm::Context {
					address: H160::zero(),
					caller: H160::zero(),
					apparent_value: sp_core::U256::zero(),
				},
			);
			handle.input = SimplePrecompileCall::view_that_writes_storage {}.encode();

			let result = SimplePrecompile::execute(&mut handle);

			assert!(matches!(result, Err(PrecompileFailure::Revert { .. })));
		});
	}

	// ========================================================================
	// Test 2: PrecompileSet with a view function
	// ========================================================================

	pub struct TxPrecompileSet;

	#[precompile_utils_macro::precompile]
	#[precompile::precompile_set]
	impl TxPrecompileSet {
		#[precompile::discriminant]
		fn discriminant(_: H160, _: u64) -> DiscriminantResult<()> {
			DiscriminantResult::Some((), 0)
		}

		#[precompile::public("writeThenRollback()")]
		#[precompile::view]
		fn write_then_rollback(_: (), _: &mut impl PrecompileHandle) -> EvmResult {
			sp_io::storage::set(STORAGE_KEY, b"mutated-by-set");
			Ok(())
		}
	}

	#[test]
	fn precompile_set_view_rolls_back_storage() {
		let mut ext = sp_io::TestExternalities::default();
		ext.execute_with(|| {
			sp_io::storage::set(STORAGE_KEY, b"original");

			TxPrecompileSet
				.prepare_test(
					[0u8; 20],
					[0u8; 20],
					TxPrecompileSetCall::write_then_rollback {},
				)
				.execute_returns(());

			assert_eq!(
				sp_io::storage::get(STORAGE_KEY).map(|b| b.to_vec()),
				Some(b"original".to_vec()),
				"PrecompileSet view function should have rolled back the storage write"
			);
		});
	}

	// ========================================================================
	// Test 3: Fallback function tagged as view
	// ========================================================================

	pub struct FallbackPrecompile;

	#[precompile_utils_macro::precompile]
	impl FallbackPrecompile {
		#[precompile::fallback]
		#[precompile::view]
		fn fallback(_handle: &mut impl PrecompileHandle) -> EvmResult {
			// This write should be rolled back because fallback is marked as view
			sp_io::storage::set(STORAGE_KEY, b"modified-by-fallback");
			Ok(())
		}
	}

	#[test]
	fn fallback_view_rolls_back_storage() {
		let mut ext = sp_io::TestExternalities::default();
		ext.execute_with(|| {
			sp_io::storage::set(STORAGE_KEY, b"original");

			// Call with unknown selector to trigger fallback
			let mut handle = crate::testing::MockHandle::new(
				H160::zero(),
				fp_evm::Context {
					address: H160::zero(),
					caller: H160::zero(),
					apparent_value: sp_core::U256::zero(),
				},
			);
			// Use a random selector that doesn't match any public function
			handle.input = vec![0xde, 0xad, 0xbe, 0xef];

			let result = FallbackPrecompile::execute(&mut handle);

			assert!(result.is_ok());
			assert_eq!(
				sp_io::storage::get(STORAGE_KEY).map(|b| b.to_vec()),
				Some(b"original".to_vec()),
				"Fallback view function should have rolled back the storage write"
			);
		});
	}
}
