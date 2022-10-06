#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible,
		tokens::fungibles::{Create, Inspect, Mutate, Transfer, Unbalanced},
		Currency, ExistenceRequirement, Get, Randomness, ReservableCurrency, WithdrawReasons,
	},
	PalletId,
};
use frame_system::{ensure_signed, pallet_prelude::*};
pub use pallet::*;

use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AccountIdConversion, Saturating, Zero},
	SaturatedConversion,
};
use sp_std::prelude::*;

use primitives::assets::AssetId;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {

	use frame_support::sp_runtime::{Perbill, Percent};

	use super::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Balances Pallet
		type Currency: Currency<Self::AccountId>
			+ ReservableCurrency<Self::AccountId>
			+ fungible::Inspect<Self::AccountId>;
		type Randomness: Randomness<Self::Hash, Self::BlockNumber>;
		#[pallet::constant]
		type ExistentialDeposit: Get<BalanceOf<Self>>;

		type ForceOrigin: EnsureOrigin<Self::Origin>;
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		type AssetManager: Create<<Self as frame_system::Config>::AccountId>
			+ Mutate<<Self as frame_system::Config>::AccountId, Balance = u128, AssetId = u128>
			+ Inspect<<Self as frame_system::Config>::AccountId>
			+ Transfer<<Self as frame_system::Config>::AccountId>
			+ Unbalanced<<Self as frame_system::Config>::AccountId>;
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
	pub struct Bet<Hash, Balance> {
		pub id: Hash,
		pub amount: Balance,
		pub bet: u128,
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
	#[scale_info(bounds(), skip_type_params(T))]
	pub struct BettingRound<T: Config> {
		pub id: Vec<u8>,
		pub start_block: T::BlockNumber,
		pub end_block: T::BlockNumber,
		pub min_bet: BalanceOf<T>,
		pub max_bet: BalanceOf<T>,
		pub total: BalanceOf<T>,
		pub winner: u128,
	}

	impl<T: Config> BettingRound<T> {
		fn from(
			id: Vec<u8>,
			start_block: T::BlockNumber,
			end_block: T::BlockNumber,
			min_bet: BalanceOf<T>,
			max_bet: BalanceOf<T>,
			amount: BalanceOf<T>,
		) -> Self {
			BettingRound { id, start_block, end_block, min_bet, max_bet, winner: 0, total: amount }
		}
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn nonce)]
	pub(super) type Nonce<T: Config> = StorageValue<_, u128, ValueQuery>;

	/// Stores betting round info
	#[pallet::storage]
	#[pallet::getter(fn get_betting_round)]
	pub(super) type InfoBettingRound<T: Config> =
		StorageMap<_, Twox64Concat, T::Hash, BettingRound<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn get_unsettled_bet)]
	pub(super) type UnsettledBetsByUser<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, T::Hash>,
			NMapKey<Twox64Concat, T::AccountId>,
			NMapKey<Twox64Concat, u128>,
		),
		BalanceOf<T>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn get_user_bet)]
	pub(super) type UsersBetBySelection<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::Hash,
		Twox64Concat,
		u128,
		Vec<T::AccountId>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn get_total_amount)]
	pub(super) type TotalAmount<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::Hash, Twox64Concat, u128, BalanceOf<T>, ValueQuery>;

	// Pallets use events to inform users when important changes are made.
	// https://docs.substrate.io/v3/runtime/events-and-errors
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
		BettingRoundRegistered(T::Hash),
		ParticipatedInRound(T::Hash, T::AccountId, u128, BalanceOf<T>),
		BettingRoundClosed(T::Hash),
		RoundRewardClaimed(T::Hash, T::AccountId),
		RoundRewardClaimFailed(T::Hash, T::AccountId, sp_runtime::DispatchError),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		CidReachedMaxSize,
		DurationMustGreaterThanZero,
		BettingRoundDoesNotExist,
		InsufficientBalance,
		NotAllowed,
		BalanceInsufficientForBettingAmount,
		NotAValidAmount,
		RoundIsClosed,
		RoundIsNotClosed,
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		#[pallet::weight((10_000, DispatchClass::Normal, Pays::No))]
		pub fn create_round(
			origin: OriginFor<T>,
			id: Vec<u8>,
			start_in: T::BlockNumber,
			duration: T::BlockNumber,
			amount: BalanceOf<T>,
			min_bet: BalanceOf<T>,
			max_bet: BalanceOf<T>,
		) -> DispatchResult {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			// https://docs.substrate.io/v3/runtime/origins
			let who = ensure_signed(origin)?;
			ensure!(id.len() <= 100, <Error<T>>::CidReachedMaxSize);
			ensure!(duration > Zero::zero(), <Error<T>>::DurationMustGreaterThanZero);
			let current_block_no = <frame_system::Pallet<T>>::block_number();
			let start_block = current_block_no.clone().saturating_add(start_in);
			let end_block = current_block_no.saturating_add(duration);
			let betting_round: BettingRound<T> =
				BettingRound::from(id, start_block, end_block, min_bet, max_bet, amount);
			let (round_id, _) = T::Randomness::random(
				&(Self::pallet_account_id(), current_block_no, who.clone(), Self::increase_nonce())
					.encode(),
			);

			let round_account_id = Self::round_account_id(round_id.clone());

			// ED Native token
			T::Currency::transfer(
				&who,
				&round_account_id,
				T::ExistentialDeposit::get(),
				ExistenceRequirement::KeepAlive,
			)?;

			// Emit an event.
			<InfoBettingRound<T>>::insert(round_id, betting_round);
			Self::deposit_event(Event::BettingRoundRegistered(round_id));
			// Return a successful DispatchResultWithPostInfo
			Ok(())
		}

		#[pallet::weight((10_000, DispatchClass::Normal, Pays::No))]
		pub fn bet(
			origin: OriginFor<T>,
			round_id: T::Hash,
			bet: u128,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let bettor_address: T::AccountId = ensure_signed(origin)?;
			ensure!(
				Self::can_withdraw(AssetId::Native, &bettor_address, amount.saturated_into())
					.is_ok(),
				Error::<T>::BalanceInsufficientForBettingAmount
			);
			ensure!(
				<InfoBettingRound<T>>::contains_key(&round_id),
				Error::<T>::BettingRoundDoesNotExist
			);
			let betting_round =
				<InfoBettingRound<T>>::get(round_id).ok_or(Error::<T>::BettingRoundDoesNotExist)?;
			let current_block_no = <frame_system::Pallet<T>>::block_number();
			ensure!(
				current_block_no >= betting_round.start_block
					&& current_block_no < betting_round.end_block,
				<Error<T>>::NotAllowed
			);
			ensure!(
				amount <= betting_round.max_bet && amount >= betting_round.min_bet,
				Error::<T>::NotAValidAmount
			);
			let round_account_id = Self::round_account_id(round_id.clone());
			Self::transfer(
				AssetId::Native,
				&bettor_address,
				&round_account_id,
				amount.saturated_into(),
			)?;

			// deposit event consider to remove
			Self::deposit_event(Event::ParticipatedInRound(
				round_id,
				bettor_address.clone(),
				bet,
				amount,
			));

			<InfoBettingRound<T>>::mutate(round_id, |v| {
				if let Some(x) = v {
					x.total = x.total.saturating_add(amount)
				}
			});

			<TotalAmount<T>>::mutate(round_id, bet, |v| *v = v.saturating_add(amount));
			<UnsettledBetsByUser<T>>::mutate((round_id, &bettor_address, bet), |v| {
				*v = v.saturating_add(amount)
			});
			<UsersBetBySelection<T>>::mutate(round_id, bet, |v| {
				if !v.iter().any(|x| x == &bettor_address) {
					v.push(bettor_address)
				}
			});

			Ok(())
		}

		#[pallet::weight((10_000, DispatchClass::Normal, Pays::No))]
		pub fn close_round(origin: OriginFor<T>, round_id: T::Hash, bet: u128) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let betting_round =
				<InfoBettingRound<T>>::get(round_id).ok_or(Error::<T>::BettingRoundDoesNotExist)?;
			let current_block_no = <frame_system::Pallet<T>>::block_number();

			ensure!(betting_round.winner == 0, Error::<T>::RoundIsClosed);

			<InfoBettingRound<T>>::mutate(round_id, |v| {
				if let Some(x) = v {
					if current_block_no < x.end_block {
						x.end_block = current_block_no;
					}
					x.winner = bet;
				}
			});
			Ok(())
		}

		#[pallet::weight((10_000, DispatchClass::Normal, Pays::No))]
		pub fn claim(
			origin: OriginFor<T>,
			bettor_address: T::AccountId,
			round_id: T::Hash,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let betting_round =
				<InfoBettingRound<T>>::get(round_id).ok_or(Error::<T>::BettingRoundDoesNotExist)?;

			ensure!(betting_round.winner != 0, Error::<T>::RoundIsNotClosed);

			let round_account_id = Self::round_account_id(round_id.clone());

			let total_amount_bet = <TotalAmount<T>>::get(round_id, betting_round.winner);

			let total_reward_u128: u128 =
				betting_round.total.saturating_sub(total_amount_bet).saturated_into();

			let user_leftover_payout = Percent::from_parts(98).mul_floor(total_reward_u128);

			let user_bet_amount =
				<UnsettledBetsByUser<T>>::get((round_id, &bettor_address, betting_round.winner));
			let user_bet_amount_u128: u128 = user_bet_amount.saturated_into();

			let user_reward_part = Perbill::from_rational(user_bet_amount, total_amount_bet);

			let user_reward = user_reward_part * user_leftover_payout + user_bet_amount_u128;

			match T::Currency::transfer(
				&round_account_id,
				&bettor_address,
				user_reward.saturated_into::<BalanceOf<T>>(),
				ExistenceRequirement::KeepAlive,
			) {
				Ok(_) => {
					<UnsettledBetsByUser<T>>::remove((
						round_id,
						&bettor_address,
						betting_round.winner,
					));
					Self::deposit_event(Event::RoundRewardClaimed(round_id, bettor_address));
				},
				Err(error) => {
					Self::deposit_event(Event::RoundRewardClaimFailed(
						round_id,
						bettor_address,
						error,
					));
				},
			};
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// module wallet account
	pub fn pallet_account_id() -> T::AccountId {
		T::PalletId::get().into_account()
	}

	/// Increments and return a nonce
	fn increase_nonce() -> u128 {
		let current_nonce: u128 = <Nonce<T>>::get();
		let (nonce, _) = current_nonce.overflowing_add(1);
		<Nonce<T>>::put(nonce);
		<Nonce<T>>::get()
	}

	/// Helper function to transfer tokens
	pub fn transfer(
		token: AssetId,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> Result<(), sp_runtime::DispatchError> {
		match token {
			AssetId::Native => T::Currency::transfer(
				from,
				to,
				amount,
				frame_support::traits::ExistenceRequirement::KeepAlive,
			),
			AssetId::Asset(token_id) => {
				T::AssetManager::transfer(token_id, &from, &to, amount.saturated_into(), false)
					.map(|_| ())
			},
		}
	}

	/// Creates an accound id from round id
	/// # Parameters
	/// * hash : Round id
	pub fn round_account_id(hash: T::Hash) -> T::AccountId {
		T::PalletId::get().into_sub_account(hash)
	}

	/// Helper function to check if investor can withdraw an amount
	pub fn can_withdraw(
		token: AssetId,
		from_account: &T::AccountId,
		amount: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		match token {
			AssetId::Native => {
				let account_free_balance: u128 =
					T::Currency::free_balance(from_account).saturated_into();
				let new_balance = account_free_balance
					.checked_sub(amount.saturated_into())
					.ok_or(Error::<T>::InsufficientBalance)?;
				T::Currency::ensure_can_withdraw(
					from_account,
					amount,
					WithdrawReasons::TRANSFER,
					new_balance.saturated_into(),
				)
			},
			AssetId::Asset(token_id) => T::AssetManager::can_withdraw(
				token_id.into(),
				from_account,
				amount.saturated_into(),
			)
			.into_result()
			.map(|_| ()),
		}
	}
}
