// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2023 Parity Technologies (UK) Ltd.
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

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::*;

use frame_support::BoundedVec;
use pallet_evm::{Precompile, PrecompileHandle, PrecompileResult, PrecompileSet};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_sha3fips::Sha3FIPS256;
use pallet_evm_precompile_simple::{ECRecover, ECRecoverPublicKey, Identity, Ripemd160, Sha256};
use scale_codec::{Decode, Encode};
use scale_info::{prelude::marker::PhantomData, TypeInfo};
use sp_core::{ConstU32, MaxEncodedLen, H160};
use sp_std::ops::Deref;

#[derive(Decode, Encode, Default, TypeInfo, Clone, PartialEq, Debug, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecompileLabel(BoundedVec<u8, ConstU32<32>>);

impl PrecompileLabel {
	pub fn new(l: BoundedVec<u8, ConstU32<32>>) -> PrecompileLabel {
		PrecompileLabel(l)
	}
}

impl Deref for PrecompileLabel {
	type Target = BoundedVec<u8, ConstU32<32>>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

pub struct OnChainPrecompiles<R>(PhantomData<R>);

impl<R> OnChainPrecompiles<R>
where
	R: pallet::Config + pallet_evm::Config,
{
	pub fn new() -> Self {
		Self(Default::default())
	}
}
impl<R> PrecompileSet for OnChainPrecompiles<R>
where
	R: pallet::Config + pallet_evm::Config,
{
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		match handle.code_address() {
			// Ethereum precompiles :
			a if &Precompiles::<R>::get(a)[..] == b"ECRecover" => Some(ECRecover::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Sha256" => Some(Sha256::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Ripemd160" => Some(Ripemd160::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Identity" => Some(Identity::execute(handle)),
			a if &Precompiles::<R>::get(a)[..] == b"Modexp" => Some(Modexp::execute(handle)),
			// Non-Frontier specific nor Ethereum precompiles :
			a if &Precompiles::<R>::get(a)[..] == b"Sha3FIPS256" => {
				Some(Sha3FIPS256::execute(handle))
			}
			a if &Precompiles::<R>::get(a)[..] == b"ECRecoverPublicKey" => {
				Some(ECRecoverPublicKey::execute(handle))
			}
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160) -> bool {
		match address {
			a if &Precompiles::<R>::get(a)[..] == b"ECRecover" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Sha256" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Ripemd160" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Identity" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Modexp" => true,
			a if &Precompiles::<R>::get(a)[..] == b"Sha3FIPS256" => true,
			a if &Precompiles::<R>::get(a)[..] == b"ECRecoverPublicKey" => true,
			_ => false,
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use crate::{PrecompileLabel, WeightInfo};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_core::H160;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin allowed to modify Precompiles
		type PrecompileModifierOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		// WeightInfo type
		type WeightInfo: WeightInfo;
	}

	#[pallet::genesis_config]
	#[cfg_attr(feature = "std", derive(Default))]
	pub struct GenesisConfig {
		pub precompiles: Vec<(H160, PrecompileLabel)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			for (address, label) in &self.precompiles {
				Pallet::<T>::do_add_precompile(address, label.clone());
			}
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn precompiles)]
	pub type Precompiles<T: Config> =
		StorageMap<_, Blake2_128Concat, H160, PrecompileLabel, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		PrecompileAdded {
			address: H160,
			label: PrecompileLabel,
		},
		PrecompileRemoved {
			address: H160,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		PrecompileDoesNotExist,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Add a precompile to storage
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::add_precompile())]
		pub fn add_precompile(
			origin: OriginFor<T>,
			address: H160,
			label: PrecompileLabel,
		) -> DispatchResult {
			T::PrecompileModifierOrigin::ensure_origin(origin)?;

			Self::do_add_precompile(&address, label.clone());

			Self::deposit_event(Event::PrecompileAdded { address, label });

			Ok(())
		}

		/// Remove a precompile from storage
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove_precompile())]
		pub fn remove_precompile(origin: OriginFor<T>, address: H160) -> DispatchResult {
			T::PrecompileModifierOrigin::ensure_origin(origin)?;

			ensure!(
				Precompiles::<T>::contains_key(address),
				Error::<T>::PrecompileDoesNotExist
			);

			Self::do_remove_precompile(&address);

			Self::deposit_event(Event::PrecompileRemoved { address });

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Add a precompile to storage
	pub fn do_add_precompile(address: &H160, label: PrecompileLabel) {
		Precompiles::<T>::set(address, label);
	}

	/// Remove a precompile from storage
	pub fn do_remove_precompile(address: &H160) {
		Precompiles::<T>::remove(address);
	}
}
