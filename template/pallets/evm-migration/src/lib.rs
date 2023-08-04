

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*; 

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_core::{H160, H256};
	use sp_runtime::traits::StaticLookup;
	use sp_std::vec::Vec;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config + pallet_balances::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type WeightInfo : WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		MigratedAccountCodes { accounts: Vec<(H160, Vec<u8>)> },
		MigratedAccountStorage { addr: H160, storage: Vec<(H256, H256)> },
		MigratedBalancesAndNonces { accounts: Vec<(T::AccountId, T::Balance, T::Index)> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// force_set_balance failed
		BalancesAndNoncesMigrationFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::migrate_account_codes(accounts.len() as u32))]
		pub fn migrate_account_codes(
			origin: OriginFor<T>,
			accounts: Vec<(H160, Vec<u8>)>,
		) -> DispatchResult {
			ensure_root(origin)?;

			for (addr, code) in accounts.clone() {
				pallet_evm::Pallet::<T>::create_account(addr, code);
			}

			Self::deposit_event(Event::MigratedAccountCodes { accounts });

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::migrate_account_storage(storage.len() as u32))]
		pub fn migrate_account_storage(
			origin: OriginFor<T>,
			addr: H160,
			storage: Vec<(H256, H256)>,
		) -> DispatchResult {
			ensure_root(origin)?;

			for (slot_index, slot_value) in storage.clone() {
				pallet_evm::AccountStorages::<T>::insert(addr, slot_index, slot_value);
			}

			Self::deposit_event(Event::MigratedAccountStorage { addr, storage });

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::migrate_account_balances_and_nonces(accounts.len() as u32))]
		pub fn migrate_account_balances_and_nonces(
			origin: OriginFor<T>,
			accounts: Vec<(T::AccountId, T::Balance, T::Index)>,
		) -> DispatchResult {
			ensure_root(origin)?;
			for (mut account, balance, nonce) in accounts.iter().cloned() {
				let who = <<T as frame_system::Config>::Lookup as StaticLookup>::unlookup(
					account.clone(),
				);
				frame_system::Account::<T>::mutate(&mut account, |a| a.nonce = nonce);
				pallet_balances::Pallet::<T>::force_set_balance(
					frame_system::RawOrigin::Root.into(),
					who,
					balance,
				)
				.map_err(|_e| Error::<T>::BalancesAndNoncesMigrationFailed)?;
			}
			Self::deposit_event(Event::MigratedBalancesAndNonces { accounts });
			Ok(())
		}
	}
}
