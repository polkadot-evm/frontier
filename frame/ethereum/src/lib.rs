// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! # Ethereum pallet
//!
//! The Ethereum pallet works together with EVM pallet to provide full emulation
//! for Ethereum block processing.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::marker::PhantomData;
use frame_support::{
	dispatch::DispatchResult, decl_module, decl_storage, decl_event,
	weights::{DispatchClass, ClassifyDispatch, WeighData, Weight, PaysFee, Pays},
};
use sp_std::prelude::*;
use frame_system::{self as system, ensure_signed, ensure_root};
use codec::{Encode, Decode};
use ethereum_types::{H160, H64, H256, U256, Bloom};
use sp_runtime::{
	traits::{
		SignedExtension, Bounded, SaturatedConversion, DispatchInfoOf, UniqueSaturatedInto,
	},
	transaction_validity::{
		ValidTransaction, TransactionValidityError, InvalidTransaction, TransactionValidity,
	},
};
use sha3::{Digest, Keccak256};

// A custom weight calculator tailored for the dispatch call `set_dummy()`. This actually examines
// the arguments and makes a decision based upon them.
//
// The `WeightData<T>` trait has access to the arguments of the dispatch that it wants to assign a
// weight to. Nonetheless, the trait itself can not make any assumptions about what the generic type
// of the arguments (`T`) is. Based on our needs, we could replace `T` with a more concrete type
// while implementing the trait. The `decl_module!` expects whatever implements `WeighData<T>` to
// replace `T` with a tuple of the dispatch arguments. This is exactly how we will craft the
// implementation below.
//
// The rules of `WeightForSetDummy` are as follows:
// - The final weight of each dispatch is calculated as the argument of the call multiplied by the
//   parameter given to the `WeightForSetDummy`'s constructor.
// - assigns a dispatch class `operational` if the argument of the call is more than 1000.
struct WeightForSetDummy<T: pallet_balances::Trait>(BalanceOf<T>);

impl<T: pallet_balances::Trait> WeighData<(&BalanceOf<T>,)> for WeightForSetDummy<T>
{
	fn weigh_data(&self, target: (&BalanceOf<T>,)) -> Weight {
		let multiplier = self.0;
		(*target.0 * multiplier).saturated_into::<Weight>()
	}
}

impl<T: pallet_balances::Trait> ClassifyDispatch<(&BalanceOf<T>,)> for WeightForSetDummy<T> {
	fn classify_dispatch(&self, target: (&BalanceOf<T>,)) -> DispatchClass {
		if *target.0 > <BalanceOf<T>>::from(1000u32) {
			DispatchClass::Operational
		} else {
			DispatchClass::Normal
		}
	}
}

impl<T: pallet_balances::Trait> PaysFee<(&BalanceOf<T>,)> for WeightForSetDummy<T> {
	fn pays_fee(&self, _target: (&BalanceOf<T>,)) -> Pays {
		Pays::Yes
	}
}

/// A type alias for the balance type from this pallet's point of view.
type BalanceOf<T> = <T as pallet_balances::Trait>::Balance;

/// Our pallet's configuration trait. All our types and constants go in here. If the
/// pallet is dependent on specific other pallets, then their configuration traits
/// should be added to our implied traits list.
///
/// `frame_system::Trait` should always be included in our implied traits.
pub trait Trait: frame_system::Trait<Hash=H256> + pallet_balances::Trait + pallet_timestamp::Trait {
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
}

decl_storage! {
	// A macro for the Storage trait, and its implementation, for this pallet.
	// This allows for type-safe usage of the Substrate storage database, so you can
	// keep things around between blocks.
	//
	// It is important to update your storage name so that your pallet's
	// storage items are isolated from other pallets.
	// ---------------------------------vvvvvvv
	trait Store for Module<T: Trait> as Example {
		BlocksAndReceipts: map hasher(blake2_128_concat) T::BlockNumber => Option<(ethereum::Block, Vec<ethereum::Receipt>)>;
		PendingTransactionsAndReceipts: Vec<(ethereum::Transaction, ethereum::Receipt)>;
	}
}

decl_event!(
	/// Events are a simple means of reporting specific conditions and
	/// circumstances that have happened that users, Dapps and/or chain explorers would find
	/// interesting and otherwise difficult to detect.
	pub enum Event<T> where B = <T as pallet_balances::Trait>::Balance {
		// Just a normal `enum`, here's a dummy event to ensure it compiles.
		/// Dummy event, just here so there's a generic type that's used.
		Dummy(B),
	}
);

// The module declaration. This states the entry points that we handle. The
// macro takes care of the marshalling of arguments and dispatch.
//
// Anyone can have these functions execute by signing and submitting
// an extrinsic. Ensure that calls into each of these execute in a time, memory and
// using storage space proportional to any costs paid for by the caller or otherwise the
// difficulty of forcing the call to happen.
//
// Generally you'll want to split these into three groups:
// - Public calls that are signed by an external account.
// - Root calls that are allowed to be made only by the governance system.
// - Unsigned calls that can be of two kinds:
//   * "Inherent extrinsics" that are opinions generally held by the block
//     authors that build child blocks.
//   * Unsigned Transactions that are of intrinsic recognizable utility to the
//     network, and are validated by the runtime.
//
// Information about where this dispatch initiated from is provided as the first argument
// "origin". As such functions must always look like:
//
// `fn foo(origin, bar: Bar, baz: Baz) -> Result;`
//
// The `Result` is required as part of the syntax (and expands to the conventional dispatch
// result of `Result<(), &'static str>`).
//
// When you come to `impl` them later in the pallet, you must specify the full type for `origin`:
//
// `fn foo(origin: T::Origin, bar: Bar, baz: Baz) { ... }`
//
// There are three entries in the `frame_system::Origin` enum that correspond
// to the above bullets: `::Signed(AccountId)`, `::Root` and `::None`. You should always match
// against them as the first thing you do in your function. There are three convenience calls
// in system that do the matching for you and return a convenient result: `ensure_signed`,
// `ensure_root` and `ensure_none`.
decl_module! {
	// Simple declaration of the `Module` type. Lets the macro know what its working on.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Deposit one of this pallet's events by using the default implementation.
		/// It is also possible to provide a custom implementation.
		/// For non-generic events, the generic parameter just needs to be dropped, so that it
		/// looks like: `fn deposit_event() = default;`.
		fn deposit_event() = default;

		// The signature could also look like: `fn on_initialize()`.
		// This function could also very well have a weight annotation, similar to any other. The
		// only difference is that it mut be returned, not annotated.
		fn on_initialize(_n: T::BlockNumber) -> Weight {
			// Anything that needs to be done at the start of the block.
			// We don't do anything here.

			0
		}

		// The signature could also look like: `fn on_finalize()`
		fn on_finalize(n: T::BlockNumber) {
			let transactions_and_receipts = PendingTransactionsAndReceipts::take();
			let (transactions, receipts): (Vec<_>, Vec<_>) =
				transactions_and_receipts.into_iter().unzip();
			let ommers = Vec::<ethereum::Header>::new();

			let header = ethereum::Header {
				parent_hash: frame_system::Module::<T>::parent_hash(),
				ommers_hash: H256::from_slice(
					Keccak256::digest(&rlp::encode_list(&ommers)[..]).as_slice(),
				), // TODO: check ommers hash.
				beneficiary: H160::default(),
				state_root: H256::default(), // TODO: figure out if there's better way to get a sort-of-valid state root.
				transactions_root: H256::from_slice(
					Keccak256::digest(&rlp::encode_list(&transactions)[..]).as_slice(),
				), // TODO: check transactions hash.
				receipts_root: H256::from_slice(
					Keccak256::digest(&rlp::encode_list(&receipts)[..]).as_slice(),
				), // TODO: check receipts hash.
				logs_bloom: Bloom::default(), // TODO: gather the logs bloom from receipts.
				difficulty: U256::zero(),
				number: U256::from(
					UniqueSaturatedInto::<u128>::unique_saturated_into(
						frame_system::Module::<T>::block_number()
					)
				),
				gas_limit: U256::zero(), // TODO: set this using Ethereum's gas limit change algorithm.
				gas_used: U256::zero(), // TODO: get this from receipts.
				timestamp: UniqueSaturatedInto::<u64>::unique_saturated_into(
					pallet_timestamp::Module::<T>::get()
				),
				extra_data: H256::default(),
				mix_hash: H256::default(),
				nonce: H64::default(),
			};

			let block = ethereum::Block {
				header,
				transactions,
				ommers,
			};

			BlocksAndReceipts::<T>::insert(n, (block, receipts));
		}

		// A runtime code run after every block and have access to extended set of APIs.
		//
		// For instance you can generate extrinsics for the upcoming produced block.
		fn offchain_worker(_n: T::BlockNumber) {
			// We don't do anything here.
			// but we could dispatch extrinsic (transaction/unsigned/inherent) using
			// sp_io::submit_extrinsic
		}
	}
}

// The main implementation block for the pallet. Functions here fall into three broad
// categories:
// - Public interface. These are functions that are `pub` and generally fall into inspector
// functions that do not write to storage and operation functions that do.
// - Private functions. These are your usual private utilities unavailable to other pallets.
impl<T: Trait> Module<T> {

}

// Similar to other FRAME pallets, your pallet can also define a signed extension and perform some
// checks and [pre/post]processing [before/after] the transaction. A signed extension can be any
// decodable type that implements `SignedExtension`. See the trait definition for the full list of
// bounds. As a convention, you can follow this approach to create an extension for your pallet:
//   - If the extension does not carry any data, then use a tuple struct with just a `marker`
//     (needed for the compiler to accept `T: Trait`) will suffice.
//   - Otherwise, create a tuple struct which contains the external data. Of course, for the entire
//     struct to be decodable, each individual item also needs to be decodable.
//
// Note that a signed extension can also indicate that a particular data must be present in the
// _signing payload_ of a transaction by providing an implementation for the `additional_signed`
// method. This example will not cover this type of extension. See `CheckRuntime` in FRAME System
// for an example.
//
// Using the extension, you can add some hooks to the life cycle of each transaction. Note that by
// default, an extension is applied to all `Call` functions (i.e. all transactions). the `Call` enum
// variant is given to each function of `SignedExtension`. Hence, you can filter based on pallet or
// a particular call if needed.
//
// Some extra information, such as encoded length, some static dispatch info like weight and the
// sender of the transaction (if signed) are also provided.
//
// The full list of hooks that can be added to a signed extension can be found
// [here](https://crates.parity.io/sp_runtime/traits/trait.SignedExtension.html).
//
// The signed extensions are aggregated in the runtime file of a substrate chain. All extensions
// should be aggregated in a tuple and passed to the `CheckedExtrinsic` and `UncheckedExtrinsic`
// types defined in the runtime. Lookup `pub type SignedExtra = (...)` in `node/runtime` and
// `node-template` for an example of this.

/// A simple signed extension that checks for the `set_dummy` call. In that case, it increases the
/// priority and prints some log.
///
/// Additionally, it drops any transaction with an encoded length higher than 200 bytes. No
/// particular reason why, just to demonstrate the power of signed extensions.
#[derive(Encode, Decode, Clone, Eq, PartialEq)]
pub struct WatchDummy<T: Trait + Send + Sync>(PhantomData<T>);

impl<T: Trait + Send + Sync> sp_std::fmt::Debug for WatchDummy<T> {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "WatchDummy")
	}
}

impl<T: Trait + Send + Sync> SignedExtension for WatchDummy<T> {
	const IDENTIFIER: &'static str = "WatchDummy";
	type AccountId = T::AccountId;
	// Note that this could also be assigned to the top-level call enum. It is passed into the
	// Balances Pallet directly and since `Trait: pallet_balances::Trait`, you could also use `T::Call`.
	// In that case, you would have had access to all call variants and could match on variants from
	// other pallets.
	type Call = Call<T>;
	type AdditionalSigned = ();
	type Pre = ();

	fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> { Ok(()) }

	fn validate(
		&self,
		_who: &Self::AccountId,
		call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> TransactionValidity {
		// if the transaction is too big, just drop it.
		if len > 200 {
			return InvalidTransaction::ExhaustsResources.into()
		}

		// check for `set_dummy`
		match call {
			_ => Ok(Default::default()),
		}
	}
}
