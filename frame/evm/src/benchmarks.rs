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
use crate::{Config, Pallet, EnsureAddressNever, EnsureAddressSame, EnsureAddressRoot,
	FeeCalculator, HashedAddressMapping, Event};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, account};
use frame_system::RawOrigin;
use frame_support::{
	assert_ok, impl_outer_origin, parameter_types, impl_outer_dispatch,
};
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    Perbill,
};
use sp_core::{U256, H256, H160, Blake2Hasher, crypto::AccountId32};
use sp_std::boxed::Box;

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

benchmarks! {
	test {
		// XXX: remove, seems to throw off macro if not present
		let x in 1..1_000_000_000;

		// moonbeam's "load-testing" contract
		let contract_bytecode = hex::decode("608060405234801561001057600080fd5b5061022b806100206000396000f3fe608060405234801561001057600080fd5b506004361061002b5760003560e01c80630f14a40614610030575b600080fd5b61004a600480360381019061004591906100b3565b610060565b60405161005791906100eb565b60405180910390f35b6000806000905060005b838110156100945760018261007f9190610106565b9150808061008c90610166565b91505061006a565b5080915050919050565b6000813590506100ad816101de565b92915050565b6000602082840312156100c557600080fd5b60006100d38482850161009e565b91505092915050565b6100e58161015c565b82525050565b600060208201905061010060008301846100dc565b92915050565b60006101118261015c565b915061011c8361015c565b9250827fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff03821115610151576101506101af565b5b828201905092915050565b6000819050919050565b60006101718261015c565b91507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8214156101a4576101a36101af565b5b600182019050919050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6101e78161015c565b81146101f257600080fd5b5056fea26469706673582212208800a18a446b2b4b898e55219e1e28f89325b6e1cd0e5c578561034c9625988a64736f6c63430008020033")
			.expect("contract bytecode decode failed");

		// const SEED: u32 = 0;
		// let caller = T::AccountId = account("caller", 0, SEED);

		let source = H160::repeat_byte(0);
		let target = H160::repeat_byte(0);

		// deploy contract
		Pallet::<T>::call(
			RawOrigin::Root.into(),
			source,
			target,
			// H160::default(), // H160 (source)
			// H160::default(), // H160 (target)
			contract_bytecode,
            U256::default(),
            1000000,
            U256::default(),
            None,
        );

	}: {
		let mut i: u32 = 0;
		loop {
			i = i+1;

			if i >= x {
				break;
			}
		}
	}
	verify {
		// assert_eq!(i, x);
	}
}

