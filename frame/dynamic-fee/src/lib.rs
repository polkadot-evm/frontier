// This file is part of Tokfin.

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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

#[cfg(test)]
mod tests;

use core::cmp::{max, min};
use frame_support::{inherent::IsFatalError, traits::Get, weights::Weight};
use sp_core::U256;
use sp_inherents::{InherentData, InherentIdentifier};

pub use self::pallet::*;
#[cfg(feature = "std")]
pub use fp_dynamic_fee::InherentDataProvider;
pub use fp_dynamic_fee::{InherentType, INHERENT_IDENTIFIER};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Bound divisor for min gas price.
		type MinGasPriceBoundDivisor: Get<U256>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			TargetMinGasPrice::<T>::kill();

			T::DbWeight::get().writes(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			if let Some(target) = TargetMinGasPrice::<T>::take() {
				let bound =
					MinGasPrice::<T>::get() / T::MinGasPriceBoundDivisor::get() + U256::one();

				let upper_limit = MinGasPrice::<T>::get().saturating_add(bound);
				let lower_limit = MinGasPrice::<T>::get().saturating_sub(bound);

				MinGasPrice::<T>::set(min(upper_limit, max(lower_limit, target)));
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Mandatory))]
		pub fn note_min_gas_price_target(origin: OriginFor<T>, target: U256) -> DispatchResult {
			ensure_none(origin)?;
			assert!(
				TargetMinGasPrice::<T>::get().is_none(),
				"TargetMinGasPrice must be updated only once in the block",
			);

			TargetMinGasPrice::<T>::set(Some(target));
			Ok(())
		}
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub min_gas_price: U256,
		#[serde(skip)]
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MinGasPrice::<T>::put(self.min_gas_price);
		}
	}

	#[pallet::storage]
	pub type MinGasPrice<T: Config> = StorageValue<_, U256, ValueQuery>;

	#[pallet::storage]
	pub type TargetMinGasPrice<T: Config> = StorageValue<_, U256>;

	#[derive(Encode, Decode, RuntimeDebug, PartialEq)]
	pub enum InherentError {
		/// The target gas price is too high compared to the current gas price.
		TargetGasPriceTooHigh,
		/// The target gas price is too low compared to the current gas price.
		TargetGasPriceTooLow,
		/// The target gas price is zero, which is not allowed.
		TargetGasPriceZero,
	}

	impl IsFatalError for InherentError {
		fn is_fatal_error(&self) -> bool {
			// All inherent errors are fatal as they indicate invalid block data
			true
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let target = data.get_data::<InherentType>(&INHERENT_IDENTIFIER).ok()??;

			Some(Call::note_min_gas_price_target { target })
		}

		fn check_inherent(call: &Self::Call, _data: &InherentData) -> Result<(), Self::Error> {
			if let Call::note_min_gas_price_target { target } = call {
				// Check that target is not zero
				if target.is_zero() {
					return Err(InherentError::TargetGasPriceZero);
				}

				// Get current gas price
				let current_gas_price = MinGasPrice::<T>::get();

				// Calculate the bound for validation
				let bound = current_gas_price / T::MinGasPriceBoundDivisor::get() + U256::one();

				// Calculate upper and lower limits for validation
				let upper_limit = current_gas_price.saturating_add(bound);
				let lower_limit = current_gas_price.saturating_sub(bound);

				// Validate that target is within the allowed bounds
				if *target > upper_limit {
					return Err(InherentError::TargetGasPriceTooHigh);
				}
				if *target < lower_limit {
					return Err(InherentError::TargetGasPriceTooLow);
				}
			}

			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::note_min_gas_price_target { .. })
		}
	}
}

impl<T: Config> fp_evm::FeeCalculator for Pallet<T> {
	fn min_gas_price() -> (U256, Weight) {
		(MinGasPrice::<T>::get(), T::DbWeight::get().reads(1))
	}
}
