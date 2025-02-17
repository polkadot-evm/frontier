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

use core::str::FromStr;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, FindAuthor},
	weights::Weight,
	ConsensusEngineId,
};
use sp_core::{H160, H256, U256};
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};

use fp_evm::{ExitError, ExitReason, Transfer};
use pallet_evm::{
	BalanceConverter, Context, EnsureAddressNever, EnsureAddressRoot, EvmBalance, FeeCalculator, IdentityAddressMapping, PrecompileHandle, SubstrateBalance
};

frame_support::construct_runtime! {
	pub enum Test {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage},
		EVM: pallet_evm::{Pallet, Call, Storage, Config<T>, Event<T>},
		Utility: pallet_utility::{Pallet, Call, Event},
	}
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
	type AccountId = H160;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = frame_system::mocking::MockBlock<Self>;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = pallet_utility::weights::SubstrateWeight<Test>;
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 0;
}
impl pallet_balances::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type WeightInfo = ();
	type Balance = u64;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxLocks = ();
	type MaxReserves = ();
	type MaxFreezes = ();
	type DoneSlashHandler = ();
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

pub struct FixedGasPrice;
impl FeeCalculator for FixedGasPrice {
	fn min_gas_price() -> (U256, Weight) {
		// Return some meaningful gas price and weight
		(1_000_000_000u128.into(), Weight::from_parts(7u64, 0))
	}
}

const EVM_DECIMALS_FACTOR: u64 = 1_000_000_000_u64;
pub struct SubtensorEvmBalanceConverter;

impl BalanceConverter for SubtensorEvmBalanceConverter {
	/// Convert from Substrate balance (u64) to EVM balance (U256)
	fn into_evm_balance(value: SubstrateBalance) -> Option<EvmBalance> {
		value
			.into_u256()
			.checked_mul(U256::from(EVM_DECIMALS_FACTOR))
			.and_then(|evm_value| {
				// Ensure the result fits within the maximum U256 value
				if evm_value <= U256::MAX {
					Some(EvmBalance::new(evm_value))
				} else {
					None
				}
			})
	}

	/// Convert from EVM balance (U256) to Substrate balance (u64)
	fn into_substrate_balance(value: EvmBalance) -> Option<SubstrateBalance> {
		value
			.into_u256()
			.checked_div(U256::from(EVM_DECIMALS_FACTOR))
			.and_then(|substrate_value| {
				// Ensure the result fits within the TAO balance type (u64)
				if substrate_value <= U256::from(u64::MAX) {
					Some(SubstrateBalance::new(substrate_value))
				} else {
					None
				}
			})
	}
}

pub struct FindAuthorTruncated;
impl FindAuthor<H160> for FindAuthorTruncated {
	fn find_author<'a, I>(_digests: I) -> Option<H160>
	where
		I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
	{
		Some(H160::from_str("1234500000000000000000000000000000000000").unwrap())
	}
}
parameter_types! {
	pub BlockGasLimit: U256 = U256::max_value();
	pub WeightPerGas: Weight = Weight::from_parts(20_000, 0);
}
impl pallet_evm::Config for Test {
	type AccountProvider = pallet_evm::FrameSystemAccountProvider<Self>;
	type FeeCalculator = FixedGasPrice;
	type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
	type WeightPerGas = WeightPerGas;

	type BlockHashMapping = pallet_evm::SubstrateBlockHashMapping<Self>;
	type CallOrigin = EnsureAddressRoot<Self::AccountId>;

	type WithdrawOrigin = EnsureAddressNever<Self::AccountId>;
	type AddressMapping = IdentityAddressMapping;
	type Currency = Balances;

	type BalanceConverter = SubtensorEvmBalanceConverter;

	type RuntimeEvent = RuntimeEvent;
	type PrecompilesType = ();
	type PrecompilesValue = ();
	type ChainId = ();
	type BlockGasLimit = BlockGasLimit;
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type OnChargeTransaction = ();
	type OnCreate = ();
	type FindAuthor = FindAuthorTruncated;
	type GasLimitPovSizeRatio = ();
	type GasLimitStorageGrowthRatio = ();
	type Timestamp = Timestamp;
	type WeightInfo = ();
}

pub(crate) struct MockHandle {
	pub input: Vec<u8>,
	pub context: Context,
}

impl PrecompileHandle for MockHandle {
	fn call(
		&mut self,
		_: H160,
		_: Option<Transfer>,
		_: Vec<u8>,
		_: Option<u64>,
		_: bool,
		_: &Context,
	) -> (ExitReason, Vec<u8>) {
		unimplemented!()
	}

	fn record_cost(&mut self, _: u64) -> Result<(), ExitError> {
		Ok(())
	}

	fn record_external_cost(
		&mut self,
		_ref_time: Option<u64>,
		_proof_size: Option<u64>,
		_storage_growth: Option<u64>,
	) -> Result<(), ExitError> {
		Ok(())
	}

	fn refund_external_cost(&mut self, _ref_time: Option<u64>, _proof_size: Option<u64>) {}

	fn remaining_gas(&self) -> u64 {
		unimplemented!()
	}

	fn log(&mut self, _: H160, _: Vec<H256>, _: Vec<u8>) -> Result<(), ExitError> {
		unimplemented!()
	}

	fn code_address(&self) -> H160 {
		unimplemented!()
	}

	fn input(&self) -> &[u8] {
		&self.input
	}

	fn context(&self) -> &Context {
		&self.context
	}

	fn is_static(&self) -> bool {
		unimplemented!()
	}

	fn gas_limit(&self) -> Option<u64> {
		None
	}
}
