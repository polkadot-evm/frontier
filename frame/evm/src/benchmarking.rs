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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::benchmarks;

type CurrencyOf<T> = <T as Config>::Currency;

benchmarks! {

	withdraw {
		let caller = frame_benchmarking::whitelisted_caller::<T::AccountId>();
		let from = H160::from_low_u64_le(0);
		let from_account_id = T::AddressMapping::into_account_id(from);
		CurrencyOf::<T>::make_free_balance_be(&from_account_id, 100_000u32.into());
	}: {
		// Withdraw should always fail with `EnsureAddressNever` WithdrawOrigin.
		let result = Pallet::<T>::withdraw(RawOrigin::Signed(caller.clone()).into(), from, 100_000u32.into());
		assert!(result.is_err());
		assert_eq!(result.unwrap_err(), sp_runtime::DispatchError::BadOrigin);
	}
}

// impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::mock::Test);
