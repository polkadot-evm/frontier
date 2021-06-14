// Copyright 2019-2021 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(feature = "runtime-benchmarks")]

//! Benchmarking
use sp_std::prelude::*;
use crate::{Config, Pallet, EnsureAddressNever, EnsureAddressSame, EnsureAddressRoot,
	FeeCalculator, HashedAddressMapping, Event};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, account};
use frame_system::RawOrigin;
use frame_support::{
	assert_ok, impl_outer_origin, parameter_types, impl_outer_dispatch,
};
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    Perbill,
};
use sp_core::{U256, H256, H160, crypto::AccountId32};
use sp_std::boxed::Box;

/*
impl_outer_origin! {
	pub enum Origin for Test where system = frame_system {}
}

/*
impl_outer_dispatch! {
	pub enum OuterCall for Test where origin: Origin {
		crate::EVM,
	}
}
*/

pub struct PalletInfo;

impl frame_support::traits::PalletInfo for PalletInfo {
	fn index<P: 'static>() -> Option<usize> {
		return Some(0)
	}

	fn name<P: 'static>() -> Option<&'static str> {
		return Some("TestName")
	}
}

#[derive(Clone, Eq, PartialEq)]
pub struct Test;
parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(1024);
}
impl frame_system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Call = ();
	type Hashing = BlakeTwo256;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = ();
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 1;
}
impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type Balance = u64;
	type DustRemoval = ();
	type Event = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
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

/// Fixed gas price of `0`.
pub struct FixedGasPrice;
impl FeeCalculator for FixedGasPrice {
	fn min_gas_price() -> U256 {
		// Gas price is always one token per gas.
		0.into()
	}
}


type System = frame_system::Module<Test>;
type Balances = pallet_balances::Module<Test>;
// type EVM = Module<Test>;

impl Config for Test {
	type FeeCalculator = FixedGasPrice;
	type GasWeightMapping = ();

	type CallOrigin = EnsureAddressRoot<Self::AccountId>;
	type WithdrawOrigin = EnsureAddressNever<Self::AccountId>;

	type AddressMapping = HashedAddressMapping<Blake2Hasher>;
	type Currency = Balances;
	type Runner = crate::runner::stack::Runner<Self>;

	type Event = Event<Test>;
	type Precompiles = ();
	type ChainId = ();
	type BlockGasLimit = ();
	type OnChargeTransaction = ();
}
*/

benchmarks! {
	test {

		// XXX: remove, seems to throw off macro if not present
		let x in 1..1_000_000_000;

		let contract_bytecode = vec![96, 128, 96, 64, 82, 52, 128, 21, 97, 0, 16, 87, 96, 0, 128, 253, 91, 80, 97, 2, 43, 128, 97, 0, 32, 96, 0, 57, 96, 0, 243, 254, 96, 128, 96, 64, 82, 52, 128, 21, 97, 0, 16, 87, 96, 0, 128, 253, 91, 80, 96, 4, 54, 16, 97, 0, 43, 87, 96, 0, 53, 96, 224, 28, 128, 99, 15, 20, 164, 6, 20, 97, 0, 48, 87, 91, 96, 0, 128, 253, 91, 97, 0, 74, 96, 4, 128, 54, 3, 129, 1, 144, 97, 0, 69, 145, 144, 97, 0, 179, 86, 91, 97, 0, 96, 86, 91, 96, 64, 81, 97, 0, 87, 145, 144, 97, 0, 235, 86, 91, 96, 64, 81, 128, 145, 3, 144, 243, 91, 96, 0, 128, 96, 0, 144, 80, 96, 0, 91, 131, 129, 16, 21, 97, 0, 148, 87, 96, 1, 130, 97, 0, 127, 145, 144, 97, 1, 6, 86, 91, 145, 80, 128, 128, 97, 0, 140, 144, 97, 1, 102, 86, 91, 145, 80, 80, 97, 0, 106, 86, 91, 80, 128, 145, 80, 80, 145, 144, 80, 86, 91, 96, 0, 129, 53, 144, 80, 97, 0, 173, 129, 97, 1, 222, 86, 91, 146, 145, 80, 80, 86, 91, 96, 0, 96, 32, 130, 132, 3, 18, 21, 97, 0, 197, 87, 96, 0, 128, 253, 91, 96, 0, 97, 0, 211, 132, 130, 133, 1, 97, 0, 158, 86, 91, 145, 80, 80, 146, 145, 80, 80, 86, 91, 97, 0, 229, 129, 97, 1, 92, 86, 91, 130, 82, 80, 80, 86, 91, 96, 0, 96, 32, 130, 1, 144, 80, 97, 1, 0, 96, 0, 131, 1, 132, 97, 0, 220, 86, 91, 146, 145, 80, 80, 86, 91, 96, 0, 97, 1, 17, 130, 97, 1, 92, 86, 91, 145, 80, 97, 1, 28, 131, 97, 1, 92, 86, 91, 146, 80, 130, 127, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 3, 130, 17, 21, 97, 1, 81, 87, 97, 1, 80, 97, 1, 175, 86, 91, 91, 130, 130, 1, 144, 80, 146, 145, 80, 80, 86, 91, 96, 0, 129, 144, 80, 145, 144, 80, 86, 91, 96, 0, 97, 1, 113, 130, 97, 1, 92, 86, 91, 145, 80, 127, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 130, 20, 21, 97, 1, 164, 87, 97, 1, 163, 97, 1, 175, 86, 91, 91, 96, 1, 130, 1, 144, 80, 145, 144, 80, 86, 91, 127, 78, 72, 123, 113, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 82, 96, 17, 96, 4, 82, 96, 36, 96, 0, 253, 91, 97, 1, 231, 129, 97, 1, 92, 86, 91, 129, 20, 97, 1, 242, 87, 96, 0, 128, 253, 91, 80, 86, 254, 162, 100, 105, 112, 102, 115, 88, 34, 18, 32, 136, 0, 161, 138, 68, 107, 43, 75, 137, 142, 85, 33, 158, 30, 40, 248, 147, 37, 182, 225, 205, 14, 92, 87, 133, 97, 3, 76, 150, 37, 152, 138, 100, 115, 111, 108, 99, 67, 0, 8, 2, 0, 51];

		let source_addr_bytes = vec![238, 139, 183, 163, 132, 239, 34, 101, 124, 51, 180, 96, 215, 171, 66, 56, 131, 9, 97, 55];
		let source = H160::from_slice(source_addr_bytes.as_slice());

		let nonce: u64 = 0;
		let nonce_as_u256: U256 = nonce.into();

		let value = U256::default();
		let gas_limit: u64 = 1250000;
		let gas_price = 1000000000.into();
		let create_dispatch_results = Pallet::<T>::create(
			RawOrigin::Root.into(),
			source,
			contract_bytecode,
			value,
			gas_limit,
			gas_price,
			Some(nonce_as_u256),
		);

		// now call deployed contract
		let contract_address_bytes = vec![129, 182, 42, 142, 233, 89, 33, 192, 197, 10, 176, 87, 246, 156, 87, 224, 182, 27, 249, 144];
		let contract_address = H160::from_slice(contract_address_bytes.as_slice());

		let encoded_call = vec![15, 20, 164, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0];

	}: {

		let nonce = nonce + 1;
		let nonce_as_u256: U256 = nonce.into();

		let call_dispatch_results = Pallet::<T>::call(
			RawOrigin::Root.into(),
			source,
			contract_address,
			encoded_call,
			value,
			gas_limit,
			gas_price,
			Some(nonce_as_u256),
        );
	}
	verify {
		assert_ok!(create_dispatch_results);
		// assert_ok!(call_dispatch_results);
	}
}

