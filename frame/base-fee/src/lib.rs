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
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::GetDefault};
	use frame_system::pallet_prelude::*;
    use sp_runtime::Permill;
    use sp_core::U256;

    pub trait BaseFeeThreshold {
        fn lower() -> Permill;
        fn upper() -> Permill;
    }

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event> + IsType<<Self as frame_system::Config>::Event>;
        /// Lower and upper bounds for increasing / decreasing `BaseFeePerGas`.
        type Threshold: BaseFeeThreshold;
        /// Coefficient used to increase or decrease `BaseFeePerGas`.
        type Modifier: Get<u32>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

    #[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub base_fee_per_gas: U256,
        _marker: PhantomData<T>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				base_fee_per_gas: U256::from(1),
                _marker: PhantomData,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			<BaseFeePerGas<T>>::put(self.base_fee_per_gas);
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn base_fee_per_gas)]
	pub type BaseFeePerGas<T> = StorageValue<_, U256, ValueQuery, GetDefault>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		NewBaseFeePerGas(U256),
	}

    #[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_n: <T as frame_system::Config>::BlockNumber) {
            let lower = T::Threshold::lower();
            let upper = T::Threshold::upper();
            let weight = <frame_system::Pallet<T>>::block_weight();
		    let max_weight = <<T as frame_system::Config>::BlockWeights as frame_support::traits::Get<_>>::get().max_block;
            // Percentage of total weight consumed by all extrinsics in the block.
            let weight_used = (weight.total().saturating_mul(100)).checked_div(max_weight).unwrap_or(100) as u32;
            // If one of the bounds is reached, update `BaseFeePerGas`.
            if Permill::from_percent(weight_used) >= upper {
                // Increase base fee by Modifier.
                let base_fee = <BaseFeePerGas<T>>::get();
                let new_base_fee = {
                    let increase = U256::from(T::Modifier::get() / 100).saturating_mul(base_fee) / 100;
                    base_fee.saturating_add(increase)
                };
                Self::deposit_event(Event::NewBaseFeePerGas(new_base_fee));
            } else if Permill::from_percent(weight_used) <= lower {
                // Decrease base fee by Modifier.
                let base_fee = <BaseFeePerGas<T>>::get();
                let new_base_fee = {
                    let decrease = U256::from(T::Modifier::get() / 100).saturating_mul(base_fee) / 100;
                    base_fee.saturating_sub(decrease)
                };
                Self::deposit_event(Event::NewBaseFeePerGas(new_base_fee));
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
	}
}
