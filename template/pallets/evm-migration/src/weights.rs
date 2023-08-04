

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet_evm_migration.
pub trait WeightInfo {
	fn migrate_account_codes(x: u32, ) -> Weight;
	fn migrate_account_balances_and_nonces(x: u32, ) -> Weight;
	fn migrate_account_storage(x: u32, ) -> Weight;
}

/// Weights for pallet_evm_migration using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: EVM AccountCodes (r:10000 w:10000)
	/// Proof Skipped: EVM AccountCodes (max_values: None, max_size: None, mode: Measured)
	/// Storage: System Account (r:10000 w:10000)
	/// Proof: System Account (max_values: None, max_size: Some(116), added: 2591, mode: MaxEncodedLen)
	/// Storage: EVM AccountCodesMetadata (r:0 w:10000)
	/// Proof Skipped: EVM AccountCodesMetadata (max_values: None, max_size: None, mode: Measured)
	/// The range of component `x` is `[1, 10000]`.
	fn migrate_account_codes(x: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `550`
		//  Estimated: `1495 + x * (2591 ±0)`
		// Minimum execution time: 40_777_000 picoseconds.
		Weight::from_parts(41_041_000, 1495)
			// Standard Error: 55_426
			.saturating_add(Weight::from_parts(44_144_384, 0).saturating_mul(x.into()))
			.saturating_add(T::DbWeight::get().reads((2_u64).saturating_mul(x.into())))
			.saturating_add(T::DbWeight::get().writes((3_u64).saturating_mul(x.into())))
			.saturating_add(Weight::from_parts(0, 2591).saturating_mul(x.into()))
	}
	/// Storage: System Account (r:10000 w:10000)
	/// Proof: System Account (max_values: None, max_size: Some(116), added: 2591, mode: MaxEncodedLen)
	/// The range of component `x` is `[1, 10000]`.
	fn migrate_account_balances_and_nonces(x: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `508`
		//  Estimated: `990 + x * (2591 ±0)`
		// Minimum execution time: 18_179_000 picoseconds.
		Weight::from_parts(18_557_000, 990)
			// Standard Error: 561_657
			.saturating_add(Weight::from_parts(42_361_748, 0).saturating_mul(x.into()))
			.saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(x.into())))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(x.into())))
			.saturating_add(Weight::from_parts(0, 2591).saturating_mul(x.into()))
	}
	/// Storage: EVM AccountStorages (r:0 w:10000)
	/// Proof Skipped: EVM AccountStorages (max_values: None, max_size: None, mode: Measured)
	/// The range of component `x` is `[1, 10000]`.
	fn migrate_account_storage(x: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_251_000 picoseconds.
		Weight::from_parts(6_416_000, 0)
			// Standard Error: 1_605
			.saturating_add(Weight::from_parts(1_259_540, 0).saturating_mul(x.into()))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(x.into())))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	/// Storage: EVM AccountCodes (r:10000 w:10000)
	/// Proof Skipped: EVM AccountCodes (max_values: None, max_size: None, mode: Measured)
	/// Storage: System Account (r:10000 w:10000)
	/// Proof: System Account (max_values: None, max_size: Some(116), added: 2591, mode: MaxEncodedLen)
	/// Storage: EVM AccountCodesMetadata (r:0 w:10000)
	/// Proof Skipped: EVM AccountCodesMetadata (max_values: None, max_size: None, mode: Measured)
	/// The range of component `x` is `[1, 10000]`.
	fn migrate_account_codes(x: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `550`
		//  Estimated: `1495 + x * (2591 ±0)`
		// Minimum execution time: 40_777_000 picoseconds.
		Weight::from_parts(41_041_000, 1495)
			// Standard Error: 55_426
			.saturating_add(Weight::from_parts(44_144_384, 0).saturating_mul(x.into()))
			.saturating_add(RocksDbWeight::get().reads((2_u64).saturating_mul(x.into())))
			.saturating_add(RocksDbWeight::get().writes((3_u64).saturating_mul(x.into())))
			.saturating_add(Weight::from_parts(0, 2591).saturating_mul(x.into()))
	}
	/// Storage: System Account (r:10000 w:10000)
	/// Proof: System Account (max_values: None, max_size: Some(116), added: 2591, mode: MaxEncodedLen)
	/// The range of component `x` is `[1, 10000]`.
	fn migrate_account_balances_and_nonces(x: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `508`
		//  Estimated: `990 + x * (2591 ±0)`
		// Minimum execution time: 18_179_000 picoseconds.
		Weight::from_parts(18_557_000, 990)
			// Standard Error: 561_657
			.saturating_add(Weight::from_parts(42_361_748, 0).saturating_mul(x.into()))
			.saturating_add(RocksDbWeight::get().reads((1_u64).saturating_mul(x.into())))
			.saturating_add(RocksDbWeight::get().writes((1_u64).saturating_mul(x.into())))
			.saturating_add(Weight::from_parts(0, 2591).saturating_mul(x.into()))
	}
	/// Storage: EVM AccountStorages (r:0 w:10000)
	/// Proof Skipped: EVM AccountStorages (max_values: None, max_size: None, mode: Measured)
	/// The range of component `x` is `[1, 10000]`.
	fn migrate_account_storage(x: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_251_000 picoseconds.
		Weight::from_parts(6_416_000, 0)
			// Standard Error: 1_605
			.saturating_add(Weight::from_parts(1_259_540, 0).saturating_mul(x.into()))
			.saturating_add(RocksDbWeight::get().writes((1_u64).saturating_mul(x.into())))
	}
}