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

pub use pallet::*;
use sp_core::U256;
use sp_inherents::{InherentData, InherentIdentifier};
#[cfg(feature = "std")]
use sp_inherents::ProvideInherentData;
use sp_std::{cmp::{max, min}, result};
#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Bound divisor for min gas price.
		#[pallet::constant]
		type MinGasPriceBoundDivisor: Get<U256>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_block_number: T::BlockNumber) {
			if let Some(target) = TargetMinGasPrice::<T>::get() {
				let bound =
					MinGasPrice::<T>::get() / T::MinGasPriceBoundDivisor::get() + U256::one();

				let upper_limit = MinGasPrice::<T>::get().saturating_add(bound);
				let lower_limit = MinGasPrice::<T>::get().saturating_sub(bound);

				MinGasPrice::<T>::set(min(upper_limit, max(lower_limit, target)));
			}

			TargetMinGasPrice::<T>::kill();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		fn note_min_gas_price_target(
			origin: OriginFor<T>,
			target: U256,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			TargetMinGasPrice::<T>::set(Some(target));
			Self::deposit_event(Event::TargetMinGasPriceSet(target));
			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		TargetMinGasPriceSet(U256),
	}

	#[pallet::storage]
	#[pallet::getter(fn min_gas_price)]
	pub(super) type MinGasPrice<T: Config> = StorageValue<_, U256, ValueQuery>;
	#[pallet::storage]
	pub(super) type TargetMinGasPrice<T: Config> = StorageValue<_, U256>;

	#[pallet::genesis_config]
	#[derive(Default)]
	pub struct GenesisConfig {
		pub min_gas_price: U256,
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			MinGasPrice::<T>::set(self.min_gas_price);
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
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
	}

	#[derive(Encode, RuntimeDebug)]
	#[cfg_attr(feature = "std", derive(Decode))]
	pub enum InherentError {}

	impl sp_inherents::IsFatalError for InherentError {
		fn is_fatal_error(&self) -> bool {
			match *self {}
		}
	}

	pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"dynfee0_";
}

impl<T: Config> pallet_evm::FeeCalculator for Pallet<T> {
	fn min_gas_price() -> U256 {
		MinGasPrice::<T>::get()
	}
}

impl InherentError {
	/// Try to create an instance ouf of the given identifier and data.
	#[cfg(feature = "std")]
	pub fn try_from(id: &InherentIdentifier, data: &[u8]) -> Option<Self> {
		if id == &INHERENT_IDENTIFIER {
			<InherentError as codec::Decode>::decode(&mut &data[..]).ok()
		} else {
			None
		}
	}
}

pub type InherentType = U256;
#[cfg(feature = "std")]
pub struct InherentDataProvider(pub InherentType);
#[cfg(feature = "std")]
impl ProvideInherentData for InherentDataProvider {
	fn inherent_identifier(&self) -> &'static InherentIdentifier {
		&INHERENT_IDENTIFIER
	}

	fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(INHERENT_IDENTIFIER, &self.0)
	}

	fn error_to_string(&self, error: &[u8]) -> Option<String> {
		InherentError::try_from(&INHERENT_IDENTIFIER, error).map(|e| format!("{:?}", e))
	}
}
