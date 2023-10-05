#![cfg_attr(not(feature = "std"), no_std)]
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(feature = "runtime-benchmarks")]
mod data;

pub mod weights;
pub use weights::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use ethereum::{LegacyTransaction, TransactionAction, TransactionSignature, TransactionV2};
	use fp_ethereum::ValidatedTransaction;
	use frame_support::{
		dispatch::{DispatchResultWithPostInfo, WithPostDispatchInfo},
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use pallet_evm::GasWeightMapping;
	use sp_core::{H160, H256, U256};
	use sp_runtime::traits::UniqueSaturatedInto;
	use fp_evm::{self, CreateInfo, CallInfo};

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config + pallet_ethereum::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	/// Authority allowed to send replay_tx extrinsics.
	#[pallet::storage]
	#[pallet::getter(fn authority)]
	pub type Authority<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	type ExecutionIndex = u64;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The transaction was successfully replayed.
		TransactionReplayed(ExecutionIndex),
		/// A call transaction reverted
		CallReverted {exection_index: ExecutionIndex, call_info: CallInfo},
		/// A Create transaction reverted 
		CreateReverted{exection_index: ExecutionIndex, create_info: CreateInfo},
		/// A new authority was set
		AuthoritySet(T::AccountId),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid signature
		InvalidSignature,
		/// The transaction failed to replay.
		TransactionReplayFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight({
			let without_base_extrinsic_weight = true;
			<T as pallet_evm::Config>::GasWeightMapping::gas_to_weight({
				gas_limit.as_u64().unique_saturated_into()
			}, without_base_extrinsic_weight)
		})]
		pub fn replay_tx(
			origin: OriginFor<T>,
			execution_index: ExecutionIndex,
			from: H160,
			nonce: U256,
			gas_price: U256,
			gas_limit: U256,
			to: Option<H160>,
			value: U256,
			data: sp_std::vec::Vec<u8>,
			v: u64,
			r: H256,
			s: H256,
		) -> DispatchResultWithPostInfo {
			let mut weight = Weight::from_parts(0, 0);

			// Note: extrinsic base weight already accounts for signature verification.
			let who = ensure_signed(origin)?;

			weight.saturating_accrue(<T as Config>::WeightInfo::is_authority());
			if !Self::is_authority(who) {
				return Err(frame_support::sp_runtime::DispatchError::BadOrigin.into());
			}

			let tx_signature =
				TransactionSignature::new(v, r, s).ok_or(Error::<T>::InvalidSignature)?;
			let tx = TransactionV2::Legacy(LegacyTransaction {
				nonce,
				gas_price,
				gas_limit,
				action: match to {
					Some(to) => TransactionAction::Call(to),
					None => TransactionAction::Create,
				},
				value,
				input: data,
				signature: tx_signature,
			});

			// consume weight for TransactionSignature::new
			weight.saturating_accrue(<T as Config>::WeightInfo::tx_creation());
			match pallet_ethereum::ValidatedTransaction::<T>::apply(from, tx) {
				Ok(tx_result) => {
					use fp_evm::CallOrCreateInfo::*;
					match tx_result.1 {
						Call(info) => {
							let exit_reason = info.exit_reason.clone();
							if exit_reason.is_succeed() {
								Self::deposit_event(Event::<T>::TransactionReplayed(
									execution_index,
								));
							} else if exit_reason.is_revert() {
								Self::deposit_event(Event::<T>::CallReverted {
									exection_index: execution_index,
									call_info: info
								}
									
								);
							} else {
								return Err(Error::<T>::TransactionReplayFailed.with_weight(weight));
							}
						},
						Create(info) => {
							let exit_reason = info.exit_reason.clone();
							if exit_reason.is_succeed() {
								Self::deposit_event(Event::<T>::TransactionReplayed(
									execution_index
								));
							} else if exit_reason.is_revert() {
								Self::deposit_event(Event::<T>::CreateReverted {
									exection_index: execution_index, create_info: info
								});
							} else {
								return Err(Error::<T>::TransactionReplayFailed.with_weight(weight));
							}
						},
					}

					if let Some(w) = tx_result.0.actual_weight {
						weight.saturating_accrue(w);
					}
				},
				Err(e) => {
					if let Some(err_weight) = e.post_info.actual_weight {
						weight.saturating_accrue(err_weight);
					}
					return Err(Error::<T>::TransactionReplayFailed.with_weight(weight));
				},
			}
			Ok(Some(weight).into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_authority())]
		pub fn set_authority(origin: OriginFor<T>, new_authority: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;
			<Authority<T>>::put(new_authority.clone());
			Self::deposit_event(Event::<T>::AuthoritySet(new_authority));
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub(crate) fn is_authority(account: T::AccountId) -> bool {
			Self::authority() == Some(account)
		}
	}
}
