// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Frontier.

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

use frame_support::{decl_module, decl_storage, decl_event, weights::Weight};
use sp_std::prelude::*;
use frame_system::{self as system, ensure_none};
use ethereum_types::{H160, H64, H256, U256, Bloom};
use sp_runtime::{
	traits::UniqueSaturatedInto,
	transaction_validity::{TransactionValidity, TransactionSource, ValidTransaction}
};
use sha3::{Digest, Keccak256};

pub use ethereum::Transaction;

/// A type alias for the balance type from this pallet's point of view.
pub type BalanceOf<T> = <T as pallet_balances::Trait>::Balance;

/// Our pallet's configuration trait. All our types and constants go in here. If the
/// pallet is dependent on specific other pallets, then their configuration traits
/// should be added to our implied traits list.
///
/// `frame_system::Trait` should always be included in our implied traits.
pub trait Trait: frame_system::Trait<Hash=H256> + pallet_balances::Trait + pallet_timestamp::Trait + pallet_evm::Trait {
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

		/// Transact an Ethereum transaction.
		#[weight = 0]
		fn transact(origin, transaction: ethereum::Transaction) {
			ensure_none(origin)?;

			let source = H160::default(); // TODO: recover sender address from transaction.
			Self::execute(source, transaction);
		}

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

impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
	type Call = Call<T>;

	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
		ValidTransaction::with_tag_prefix("Ethereum")
			.and_provides(call)
			.build()
	}
}

// The main implementation block for the pallet. Functions here fall into three broad
// categories:
// - Public interface. These are functions that are `pub` and generally fall into inspector
// functions that do not write to storage and operation functions that do.
// - Private functions. These are your usual private utilities unavailable to other pallets.
impl<T: Trait> Module<T> {
	/// Execute an Ethereum transaction, ignoring transaction signatures.
	pub fn execute(source: H160, transaction: ethereum::Transaction) {
		match transaction.action {
			ethereum::TransactionAction::Call(target) => {
				pallet_evm::Module::<T>::execute_call(
					source,
					target,
					transaction.input.clone(),
					transaction.value,
					transaction.gas_limit.low_u32(),
					transaction.gas_price,
					Some(transaction.nonce),
				).unwrap(); // TODO: handle error
			},
			ethereum::TransactionAction::Create => {
				pallet_evm::Module::<T>::execute_create(
					source,
					transaction.input.clone(),
					transaction.value,
					transaction.gas_limit.low_u32(),
					transaction.gas_price,
					Some(transaction.nonce),
				).unwrap(); // TODO: handle error
			},
		}

		let receipt = ethereum::Receipt {
			state_root: H256::default(), // TODO: should be okay / error status.
			used_gas: U256::default(), // TODO: set this.
			logs_bloom: Bloom::default(), // TODO: set this.
			logs: Vec::new(), // TODO: set this.
		};

		PendingTransactionsAndReceipts::append((transaction, receipt));
	}
}
