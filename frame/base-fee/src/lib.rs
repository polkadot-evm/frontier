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
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use sp_core::U256;
	use sp_runtime::Permill;

	pub trait BaseFeeThreshold {
		fn lower() -> Permill;
		fn upper() -> Permill;
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event> + IsType<<Self as frame_system::Config>::Event>;
		/// Lower and upper bounds for increasing / decreasing `BaseFeePerGas`.
		type Threshold: BaseFeeThreshold;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub base_fee_per_gas: U256,
		pub is_active: bool,
		pub elasticity: Permill,
		_marker: PhantomData<T>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> GenesisConfig<T> {
		pub fn new(base_fee_per_gas: U256, is_active: bool, elasticity: Permill) -> Self {
			Self {
				base_fee_per_gas,
				is_active,
				elasticity,
				_marker: PhantomData,
			}
		}
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				// 1 GWEI
				base_fee_per_gas: U256::from(1_000_000_000),
				is_active: true,
				elasticity: Permill::from_parts(125_000),
				_marker: PhantomData,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			<BaseFeePerGas<T>>::put(self.base_fee_per_gas);
			<IsActive<T>>::put(self.is_active);
		}
	}

	#[pallet::type_value]
	pub fn DefaultBaseFeePerGas() -> U256 {
		U256::from(1_000_000_000)
	}

	#[pallet::storage]
	#[pallet::getter(fn base_fee_per_gas)]
	pub type BaseFeePerGas<T> = StorageValue<_, U256, ValueQuery, DefaultBaseFeePerGas>;

	#[pallet::type_value]
	pub fn DefaultIsActive() -> bool {
		true
	}

	#[pallet::storage]
	#[pallet::getter(fn is_active)]
	pub type IsActive<T> = StorageValue<_, bool, ValueQuery, DefaultIsActive>;

	#[pallet::type_value]
	pub fn DefaultElasticity() -> Permill {
		Permill::from_parts(125_000)
	}

	#[pallet::storage]
	#[pallet::getter(fn elasticity)]
	pub type Elasticity<T> = StorageValue<_, Permill, ValueQuery, DefaultElasticity>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		NewBaseFeePerGas(U256),
		BaseFeeOverflow,
		IsActive(bool),
		NewElasticity(Permill),
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: T::BlockNumber) -> Weight {
			// Register the Weight used on_finalize.
			// 	- One storage read to get the block_weight.
			// 	- One storage read to get the Elasticity.
			// 	- One write to BaseFeePerGas.
			let db_weight =
				<<T as frame_system::Config>::DbWeight as frame_support::traits::Get<_>>::get();
			db_weight.reads(2).saturating_add(db_weight.write)
		}

		fn on_finalize(_n: <T as frame_system::Config>::BlockNumber) {
			if <IsActive<T>>::get() {
				let lower = T::Threshold::lower();
				let upper = T::Threshold::upper();
				// `target` is the ideal congestion of the network where the base fee should remain unchanged.
				// Under normal circumstances the `target` should be 50%.
				// If we go below the `target`, the base fee is linearly decreased by the Elasticity delta of lower~target.
				// If we go above the `target`, the base fee is linearly increased by the Elasticity delta of upper~target.
				// The base fee is fully increased (default 12.5%) if the block is upper full (default 100%).
				// The base fee is fully decreased (default 12.5%) if the block is lower empty (default 0%).
				let weight = <frame_system::Pallet<T>>::block_weight();
				let max_weight =
					<<T as frame_system::Config>::BlockWeights as frame_support::traits::Get<_>>::get()
						.max_block;

				// We convert `weight` into block fullness and ensure we are within the lower and upper bound.
				let weight_used =
					Permill::from_rational(weight.total(), max_weight).clamp(lower, upper);
				// After clamp `weighted_used` is always between `lower` and `upper`.
				// We scale the block fullness range to the lower/upper range, and the usage represents the
				// actual percentage within this new scale.
				let usage = (weight_used - lower) / (upper - lower);

				// 50% block fullness is our threshold.
				let target = Permill::from_parts(500_000);
				if usage > target {
					// Above target, increase.
					let coef =
						Permill::from_parts((usage.deconstruct() - target.deconstruct()) * 2u32);
					// How much of the Elasticity is used to mutate base fee.
					let coef = <Elasticity<T>>::get() * coef;
					<BaseFeePerGas<T>>::mutate(|bf| {
						if let Some(scaled_basefee) = bf.checked_mul(U256::from(coef.deconstruct()))
						{
							// Normalize to GWEI.
							let increase = scaled_basefee
								.checked_div(U256::from(1_000_000))
								.unwrap_or(U256::zero());
							*bf = bf.saturating_add(U256::from(increase));
							Self::deposit_event(Event::NewBaseFeePerGas(*bf));
						} else {
							Self::deposit_event(Event::BaseFeeOverflow);
						}
					});
				} else if usage < target {
					// Below target, decrease.
					let coef =
						Permill::from_parts((target.deconstruct() - usage.deconstruct()) * 2u32);
					// How much of the Elasticity is used to mutate base fee.
					let coef = <Elasticity<T>>::get() * coef;
					<BaseFeePerGas<T>>::mutate(|bf| {
						if let Some(scaled_basefee) = bf.checked_mul(U256::from(coef.deconstruct()))
						{
							// Normalize to GWEI.
							let decrease = scaled_basefee
								.checked_div(U256::from(1_000_000))
								.unwrap_or(U256::zero());
							*bf = bf.saturating_sub(U256::from(decrease));
							Self::deposit_event(Event::NewBaseFeePerGas(*bf));
						} else {
							Self::deposit_event(Event::BaseFeeOverflow);
						}
					});
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn set_base_fee_per_gas(origin: OriginFor<T>, fee: U256) -> DispatchResult {
			ensure_root(origin)?;
			<BaseFeePerGas<T>>::put(fee);
			Self::deposit_event(Event::NewBaseFeePerGas(fee));
			Ok(())
		}

		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn set_is_active(origin: OriginFor<T>, is_active: bool) -> DispatchResult {
			ensure_root(origin)?;
			<IsActive<T>>::put(is_active);
			Self::deposit_event(Event::IsActive(is_active));
			Ok(())
		}

		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn set_elasticity(origin: OriginFor<T>, elasticity: Permill) -> DispatchResult {
			ensure_root(origin)?;
			<Elasticity<T>>::put(elasticity);
			Self::deposit_event(Event::NewElasticity(elasticity));
			Ok(())
		}
	}

	impl<T: Config> pallet_evm::FeeCalculator for Pallet<T> {
		fn min_gas_price() -> U256 {
			<BaseFeePerGas<T>>::get()
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate as pallet_base_fee;

	use frame_support::{
		assert_ok, pallet_prelude::GenesisBuild, parameter_types, traits::OnFinalize,
		weights::DispatchClass,
	};
	use sp_core::{H256, U256};
	use sp_io::TestExternalities;
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
		Permill,
	};

	pub fn new_test_ext(base_fee: U256) -> TestExternalities {
		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<Test>()
			.unwrap();

		pallet_base_fee::GenesisConfig::<Test>::new(base_fee, true, Permill::from_parts(125_000))
			.assimilate_storage(&mut t)
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
		type BaseCallFilter = frame_support::traits::Everything;
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
		pub const Threshold: (u8, u8) = (0, 100);
	}

	pub struct BaseFeeThreshold;
	impl pallet_base_fee::BaseFeeThreshold for BaseFeeThreshold {
		fn lower() -> Permill {
			Permill::zero()
		}
		fn upper() -> Permill {
			Permill::from_parts(1_000_000)
		}
	}

	impl Config for Test {
		type Event = Event;
		type Threshold = BaseFeeThreshold;
	}

	frame_support::construct_runtime!(
		pub enum Test where
			Block = Block,
			NodeBlock = Block,
			UncheckedExtrinsic = UncheckedExtrinsic,
		{
			System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
			BaseFee: pallet_base_fee::{Pallet, Call, Storage, Event},
		}
	);

	#[test]
	fn should_not_overflow_u256() {
		let base_fee = U256::max_value();
		new_test_ext(base_fee).execute_with(|| {
			let init = BaseFee::base_fee_per_gas();
			System::register_extra_weight_unchecked(1000000000000, DispatchClass::Normal);
			BaseFee::on_finalize(System::block_number());
			assert_eq!(BaseFee::base_fee_per_gas(), init);
		});
	}

	#[test]
	fn should_handle_zero() {
		let base_fee = U256::zero();
		new_test_ext(base_fee).execute_with(|| {
			let init = BaseFee::base_fee_per_gas();
			BaseFee::on_finalize(System::block_number());
			assert_eq!(BaseFee::base_fee_per_gas(), init);
		});
	}

	#[test]
	fn should_handle_consecutive_empty_blocks() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			for _ in 0..10000 {
				BaseFee::on_finalize(System::block_number());
				System::set_block_number(System::block_number() + 1);
			}
			assert_eq!(
				BaseFee::base_fee_per_gas(),
				// 8 is the lowest number which's 12.5% is >= 1.
				U256::from(7)
			);
		});
	}

	#[test]
	fn should_handle_consecutive_full_blocks() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			for _ in 0..10000 {
				// Register max weight in block.
				System::register_extra_weight_unchecked(1000000000000, DispatchClass::Normal);
				BaseFee::on_finalize(System::block_number());
				System::set_block_number(System::block_number() + 1);
			}
			assert_eq!(
				BaseFee::base_fee_per_gas(),
				// Max value allowed in the algorithm before overflowing U256.
				U256::from_dec_str(
					"930583037201699994746877284806656508753618758732556029383742480470471799"
				)
				.unwrap()
			);
		});
	}

	#[test]
	fn should_increase_total_base_fee() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1000000000));
			// Register max weight in block.
			System::register_extra_weight_unchecked(1000000000000, DispatchClass::Normal);
			BaseFee::on_finalize(System::block_number());
			// Expect the base fee to increase by 12.5%.
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1125000000));
		});
	}

	#[test]
	fn should_increase_delta_of_base_fee() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1000000000));
			// Register 75% capacity in block weight.
			System::register_extra_weight_unchecked(750000000000, DispatchClass::Normal);
			BaseFee::on_finalize(System::block_number());
			// Expect a 6.25% increase in base fee for a target capacity of 50% ((75/50)-1 = 0.5 * 0.125 = 0.0625).
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1062500000));
		});
	}

	#[test]
	fn should_idle_base_fee() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1000000000));
			// Register half capacity in block weight.
			System::register_extra_weight_unchecked(500000000000, DispatchClass::Normal);
			BaseFee::on_finalize(System::block_number());
			// Expect the base fee to remain unchanged
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1000000000));
		});
	}

	#[test]
	fn set_base_fee_per_gas_dispatchable() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1000000000));
			assert_ok!(BaseFee::set_base_fee_per_gas(Origin::root(), U256::from(1)));
			assert_eq!(BaseFee::base_fee_per_gas(), U256::from(1));
		});
	}

	#[test]
	fn set_is_active_dispatchable() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			assert_eq!(BaseFee::is_active(), true);
			assert_ok!(BaseFee::set_is_active(Origin::root(), false));
			assert_eq!(BaseFee::is_active(), false);
		});
	}

	#[test]
	fn set_elasticity_dispatchable() {
		let base_fee = U256::from(1_000_000_000);
		new_test_ext(base_fee).execute_with(|| {
			assert_eq!(BaseFee::elasticity(), Permill::from_parts(125_000));
			assert_ok!(BaseFee::set_elasticity(
				Origin::root(),
				Permill::from_parts(1_000)
			));
			assert_eq!(BaseFee::elasticity(), Permill::from_parts(1_000));
		});
	}
}
