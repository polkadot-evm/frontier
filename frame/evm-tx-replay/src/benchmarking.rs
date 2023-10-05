//! Benchmarking setup for pallet-evm-tx-replay
#![cfg(feature = "runtime-benchmarks")]
use super::*;

use crate::Pallet as EvmTxReplay;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_std::vec;

#[benchmarks]
pub mod benchmarks {
	use super::*;

	#[benchmark]
	pub fn set_authority() {
		let authority: T::AccountId = account("authority", 0, 1);

		#[extrinsic_call]
		_(RawOrigin::Root, authority.clone());

		// Verify
		let storage_authority = crate::Authority::<T>::get();
		assert_eq!(storage_authority, Some(authority.clone()));
		assert_last_event::<T>(<Event<T>>::AuthoritySet(authority).into());
	}

	#[benchmark]
	pub fn is_authority() {
		#[block]
		{
			let _ = EvmTxReplay::<T>::is_authority(account("authority", 0, 1));
		}
	}

	#[benchmark]
	fn tx_creation() {
		use data::TestTransaction;
		let TestTransaction { nonce, gas_price, gas_limit, value, data, v, r, s, to, .. } =
			TestTransaction::get_sample();
		#[block]
		{
			// tx creation
			let tx_signature = ethereum::TransactionSignature::new(v, r, s)
				.ok_or(Error::<T>::InvalidSignature)
				.expect("Expected valid sig");
			let _tx = ethereum::TransactionV2::Legacy(ethereum::LegacyTransaction {
				nonce,
				gas_price,
				gas_limit,
				action: match to {
					Some(to) => ethereum::TransactionAction::Call(to),
					None => ethereum::TransactionAction::Create,
				},
				value,
				input: data,
				signature: tx_signature,
			});
		}
	}
}

impl_benchmark_test_suite!(EvmTxReplay, crate::mock::new_test_ext(), crate::mock::Test,);

fn assert_last_event<T: Config>(e: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(e.into());
}
