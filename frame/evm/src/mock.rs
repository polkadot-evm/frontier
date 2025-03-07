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

//! Test mock for unit tests and benchmarking

use frame_support::{derive_impl, parameter_types, weights::Weight};
use sp_core::{H160, U256};

use crate::{
    EnsureAddressNever, EnsureAddressRoot, FeeCalculator, IsPrecompileResult, Precompile, PrecompileHandle, PrecompileResult,
	PrecompileSet, IdentityAddressMapping, EnsureAllowedCreateAddress
};

frame_support::construct_runtime! {
	pub enum Test {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage},
		EVM: crate::{Pallet, Call, Storage, Config<T>, Event<T>},
	}
}

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(Weight::from_parts(1024, 0));
}

#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Block = frame_system::mocking::MockBlock<Self>;
	type BlockHashCount = BlockHashCount;
	type AccountData = pallet_balances::AccountData<u64>;
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 0;
}
#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {}

parameter_types! {
	pub MockPrecompiles: MockPrecompileSet = MockPrecompileSet;
	pub SuicideQuickClearLimit: u32 = 0;
	pub AllowedAddressesCreate: Vec<H160> = vec![H160::default(), H160::from([4u8;20])];
	pub AllowedAddressesCreateInner: Vec<H160> = vec![H160::from([4u8;20]), H160::from([5u8;20])];
}

#[derive_impl(crate::config_preludes::TestDefaultConfig)]
impl crate::Config for Test {
	type AccountProvider = crate::FrameSystemAccountProvider<Self>;
	type FeeCalculator = FixedGasPrice;
	type BlockHashMapping = crate::SubstrateBlockHashMapping<Self>;
	type CallOrigin = EnsureAddressRoot<Self::AccountId>;
	type CreateOrigin = EnsureAllowedCreateAddress<AllowedAddressesCreate>;
	type CreateInnerOrigin = EnsureAllowedCreateAddress<AllowedAddressesCreateInner>;

	type WithdrawOrigin = EnsureAddressNever<Self::AccountId>;
	type AddressMapping = IdentityAddressMapping;
	type Currency = Balances;
	type PrecompilesType = MockPrecompileSet;
	type PrecompilesValue = MockPrecompiles;
	type Runner = crate::runner::stack::Runner<Self>;
	type Timestamp = Timestamp;
}

pub struct FixedGasPrice;
impl FeeCalculator for FixedGasPrice {
	fn min_gas_price() -> (U256, Weight) {
		// Return some meaningful gas price and weight
		(1_000_000_000u128.into(), Weight::from_parts(7u64, 0))
	}
}

/// Example PrecompileSet with only Identity precompile.
pub struct MockPrecompileSet;

impl PrecompileSet for MockPrecompileSet {
	/// Tries to execute a precompile in the precompile set.
	/// If the provided address is not a precompile, returns None.
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		let address = handle.code_address();

		if address == H160::from_low_u64_be(1) {
			return Some(pallet_evm_precompile_simple::Identity::execute(handle));
		}

		None
	}

	/// Check if the given address is a precompile. Should only be called to
	/// perform the check while not executing the precompile afterward, since
	/// `execute` already performs a check internally.
	fn is_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: address == H160::from_low_u64_be(1),
			extra_cost: 0,
		}
	}
}
