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
use frame_support::inherent::IsFatalError;
use sp_core::U256;
use sp_inherents::{InherentData, InherentIdentifier};
use sp_std::{
	cmp::{max, min},
	result,
};

pub use pallet::*;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

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
	pub struct GenesisConfig {
		pub min_gas_price: U256,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
				min_gas_price: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			MinGasPrice::<T>::put(self.min_gas_price);
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn min_gas_price)]
	pub(super) type MinGasPrice<T: Config> = StorageValue<_, U256, ValueQuery>;

	#[pallet::storage]
	pub(super) type TargetMinGasPrice<T: Config> = StorageValue<_, U256>;

	#[derive(Encode, Decode, RuntimeDebug)]
	pub enum InherentError {}

	impl IsFatalError for InherentError {
		fn is_fatal_error(&self) -> bool {
			match *self {}
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

		fn check_inherent(
			_call: &Self::Call,
			_data: &InherentData,
		) -> result::Result<(), Self::Error> {
			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::note_min_gas_price_target { .. })
		}
	}
}

impl<T: Config> pallet_evm::FeeCalculator for Pallet<T> {
	fn min_gas_price() -> U256 {
		MinGasPrice::<T>::get()
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
