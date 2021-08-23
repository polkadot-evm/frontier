// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2021 Parity Technologies (UK) Ltd.
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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use async_trait::async_trait;
use codec::{Decode, Encode};
use frame_support::{
	decl_module, decl_storage,
	inherent::{IsFatalError, ProvideInherent},
	traits::Get,
	weights::{DispatchClass, Weight},
};
use frame_system::ensure_none;
use sp_core::U256;
use sp_inherents::{InherentData, InherentIdentifier};
use sp_runtime::RuntimeDebug;
use sp_std::{
	cmp::{max, min},
	result,
};

pub trait Config: frame_system::Config {
	/// Bound divisor for min gas price.
	type MinGasPriceBoundDivisor: Get<U256>;
}

decl_storage! {
	trait Store for Module<T: Config> as DynamicFee {
		MinGasPrice get(fn min_gas_price) config(): U256;
		TargetMinGasPrice: Option<U256>;
	}
	add_extra_genesis {
		build(|_config: &GenesisConfig| {
			MinGasPrice::set(U256::from(1));
		});
	}
}

decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		fn on_initialize(_block_number: T::BlockNumber) -> Weight {
			TargetMinGasPrice::kill();

			T::DbWeight::get().writes(1)
		}

		fn on_finalize(_n: T::BlockNumber) {
			if let Some(target) = TargetMinGasPrice::take() {
				let bound = MinGasPrice::get() / T::MinGasPriceBoundDivisor::get() + U256::one();

				let upper_limit = MinGasPrice::get().saturating_add(bound);
				let lower_limit = MinGasPrice::get().saturating_sub(bound);

				MinGasPrice::set(min(upper_limit, max(lower_limit, target)));
			}
		}

		#[weight = (T::DbWeight::get().writes(1), DispatchClass::Mandatory)]
		pub fn note_min_gas_price_target(
			origin,
			target: U256,
		) {
			ensure_none(origin)?;
			assert!(TargetMinGasPrice::get().is_none(), "TargetMinGasPrice must be updated only once in the block");

			TargetMinGasPrice::set(Some(target));
		}
	}
}

impl<T: Config> pallet_evm::FeeCalculator for Module<T> {
	fn min_gas_price() -> U256 {
		MinGasPrice::get()
	}
}

impl<T: Config> ProvideInherent for Module<T> {
	type Call = Call<T>;
	type Error = InherentError;
	const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

	fn create_inherent(data: &InherentData) -> Option<Self::Call> {
		let target = data.get_data::<InherentType>(&INHERENT_IDENTIFIER).ok()??;

		Some(Call::note_min_gas_price_target(target))
	}

	fn check_inherent(_call: &Self::Call, _data: &InherentData) -> result::Result<(), Self::Error> {
		Ok(())
	}

	fn is_inherent(call: &Self::Call) -> bool {
		matches!(call, Call::note_min_gas_price_target(_))
	}
}

#[derive(Encode, Decode, RuntimeDebug)]
pub enum InherentError {}

impl IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		match *self {}
	}
}

pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"dynfee0_";

pub type InherentType = U256;

#[cfg(feature = "std")]
pub struct InherentDataProvider(pub InherentType);

#[cfg(feature = "std")]
#[async_trait]
impl sp_inherents::InherentDataProvider for InherentDataProvider {
	fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(INHERENT_IDENTIFIER, &self.0)
	}

	async fn try_handle_error(
		&self,
		_identifier: &InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		// The pallet never reports error.
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate as pallet_dynamic_fee;

	use frame_support::{
		assert_ok, parameter_types,
		traits::{OnFinalize, OnInitialize},
	};
	use sp_core::{H256, U256};
	use sp_io::TestExternalities;
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
	};

	pub fn new_test_ext() -> TestExternalities {
		let t = frame_system::GenesisConfig::default()
			.build_storage::<Test>()
			.unwrap();
		TestExternalities::new(t)
	}

	type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
	type Block = frame_system::mocking::MockBlock<Test>;

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
		type Call = Call;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = Event;
		type BlockHashCount = BlockHashCount;
		type Version = ();
		type PalletInfo = PalletInfo;
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ();
		type OnSetCode = ();
	}

	frame_support::parameter_types! {
		pub const MinimumPeriod: u64 = 1000;
	}
	impl pallet_timestamp::Config for Test {
		type Moment = u64;
		type OnTimestampSet = ();
		type MinimumPeriod = MinimumPeriod;
		type WeightInfo = ();
	}

	frame_support::parameter_types! {
		pub BoundDivision: U256 = 1024.into();
	}
	impl Config for Test {
		type MinGasPriceBoundDivisor = BoundDivision;
	}

	frame_support::construct_runtime!(
		pub enum Test where
			Block = Block,
			NodeBlock = Block,
			UncheckedExtrinsic = UncheckedExtrinsic,
		{
			System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
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
				Origin::none(),
				U256::zero()
			));
			let _ = DynamicFee::note_min_gas_price_target(Origin::none(), U256::zero());
			run_to_block(4);
			assert_ok!(DynamicFee::note_min_gas_price_target(
				Origin::none(),
				U256::zero()
			));
		});
	}
}
