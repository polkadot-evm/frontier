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

use super::*;
use crate as pallet_dynamic_fee;

use frame_support::{
	assert_err, assert_ok, derive_impl,
	pallet_prelude::ProvideInherent,
	parameter_types,
	traits::{ConstU32, OnFinalize, OnInitialize},
	weights::Weight,
};
use sp_core::{H256, U256};
use sp_io::TestExternalities;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

pub fn new_test_ext() -> TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();
	TestExternalities::new(t)
}

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(Weight::from_parts(1024, 0));
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = RuntimeTask;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = frame_system::mocking::MockBlock<Self>;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = 1000;
}
impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

parameter_types! {
	pub BoundDivision: U256 = 1024.into();
}
impl Config for Test {
	type MinGasPriceBoundDivisor = BoundDivision;
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage},
		DynamicFee: pallet_dynamic_fee::{Pallet, Call, Storage, Inherent},
	}
);

fn run_to_block(n: u64) {
	while System::block_number() < n {
		DynamicFee::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		DynamicFee::on_initialize(System::block_number());
	}
}

#[test]
#[should_panic(expected = "TargetMinGasPrice must be updated only once in the block")]
fn double_set_in_a_block_failed() {
	new_test_ext().execute_with(|| {
		run_to_block(3);
		assert_ok!(DynamicFee::note_min_gas_price_target(
			RuntimeOrigin::none(),
			U256::zero()
		));
		let _ = DynamicFee::note_min_gas_price_target(RuntimeOrigin::none(), U256::zero());
		run_to_block(4);
		assert_ok!(DynamicFee::note_min_gas_price_target(
			RuntimeOrigin::none(),
			U256::zero()
		));
	});
}

#[test]
fn check_inherent_validates_target_gas_price() {
	new_test_ext().execute_with(|| {
		// Set initial gas price
		MinGasPrice::<Test>::put(U256::from(1000));

		// With BoundDivision = 1024, the bound should be 1000/1024 + 1 = 1
		// So upper limit = 1000 + 1 = 1001, lower limit = 1000 - 1 = 999

		// Test valid target (within bounds)
		let valid_target = U256::from(1000); // Current price is valid
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: valid_target,
		};
		assert_ok!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			)
		);

		// Test target too high
		let too_high_target = U256::from(1002); // Above upper bound of 1001
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: too_high_target,
		};
		assert_err!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			),
			pallet_dynamic_fee::InherentError::TargetGasPriceTooHigh
		);

		// Test target too low
		let too_low_target = U256::from(998); // Below lower bound of 999
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: too_low_target,
		};
		assert_err!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			),
			pallet_dynamic_fee::InherentError::TargetGasPriceTooLow
		);

		// Test zero target
		let zero_target = U256::zero();
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: zero_target,
		};
		assert_err!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			),
			pallet_dynamic_fee::InherentError::TargetGasPriceZero
		);
	});
}

#[test]
fn check_inherent_bounds_calculation() {
	new_test_ext().execute_with(|| {
		// Set initial gas price
		MinGasPrice::<Test>::put(U256::from(1000));

		// With BoundDivision = 1024, the bound should be 1000/1024 + 1 = 1
		// So upper limit = 1000 + 1 = 1001, lower limit = 1000 - 1 = 999

		// Test exact upper bound
		let upper_bound_target = U256::from(1001);
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: upper_bound_target,
		};
		assert_ok!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			)
		);

		// Test exact lower bound
		let lower_bound_target = U256::from(999);
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: lower_bound_target,
		};
		assert_ok!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			)
		);

		// Test just above upper bound
		let just_above_target = U256::from(1002);
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: just_above_target,
		};
		assert_err!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			),
			pallet_dynamic_fee::InherentError::TargetGasPriceTooHigh
		);

		// Test just below lower bound
		let just_below_target = U256::from(998);
		let call = pallet_dynamic_fee::Call::note_min_gas_price_target {
			target: just_below_target,
		};
		assert_err!(
			<pallet_dynamic_fee::Pallet::<Test> as ProvideInherent>::check_inherent(
				&call,
				&sp_inherents::InherentData::new()
			),
			pallet_dynamic_fee::InherentError::TargetGasPriceTooLow
		);
	});
}
