// This file is part of Frontier.

// Copyright (C) Frontier developers.
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

//! # PolkaVM support for EVM Pallet

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

extern crate alloc;

mod vm;
mod weights;

pub use self::{pallet::*, weights::WeightInfo};

#[frame_support::pallet]
pub mod pallet {
	use super::WeightInfo;
	use frame_support::pallet_prelude::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
		type WeightInfo: WeightInfo;
	}

	impl<T: Config> Get<u64> for Pallet<T> {
		fn get() -> u64 {
			<ChainId<T>>::get()
		}
	}

	/// The EVM chain ID.
	#[pallet::storage]
	pub type ChainId<T> = StorageValue<_, u64, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub chain_id: u64,
		#[serde(skip)]
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			ChainId::<T>::put(self.chain_id);
		}
	}
}
