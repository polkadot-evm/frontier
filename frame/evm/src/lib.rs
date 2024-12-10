// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! # EVM Pallet
//!
//! The EVM pallet allows unmodified EVM code to be executed in a Substrate-based blockchain.
//! - [`evm::Config`]
//!
//! ## EVM Engine
//!
//! The EVM pallet uses [`SputnikVM`](https://github.com/rust-blockchain/evm) as the underlying EVM engine.
//! The engine is overhauled so that it's [`modular`](https://github.com/corepaper/evm).
//!
//! ## Execution Lifecycle
//!
//! There are a separate set of accounts managed by the EVM pallet. Substrate based accounts can call the EVM Pallet
//! to deposit or withdraw balance from the Substrate base-currency into a different balance managed and used by
//! the EVM pallet. Once a user has populated their balance, they can create and call smart contracts using this pallet.
//!
//! There's one-to-one mapping from Substrate accounts and EVM external accounts that is defined by a conversion function.
//!
//! ## EVM Pallet vs Ethereum Network
//!
//! The EVM pallet should be able to produce nearly identical results compared to the Ethereum mainnet,
//! including gas cost and balance changes.
//!
//! Observable differences include:
//!
//! - The available length of block hashes may not be 256 depending on the configuration of the System pallet
//! in the Substrate runtime.
//! - Difficulty and coinbase, which do not make sense in this pallet and is currently hard coded to zero.
//!
//! We currently do not aim to make unobservable behaviors, such as state root, to be the same. We also don't aim to follow
//! the exact same transaction / receipt format. However, given one Ethereum transaction and one Substrate account's
//! private key, one should be able to convert any Ethereum transaction into a transaction compatible with this pallet.
//!
//! The gas configurations are configurable. Right now, a pre-defined London hard fork configuration option is provided.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]
#![allow(clippy::too_many_arguments)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod mock;
pub mod runner;
#[cfg(test)]
mod tests;
pub mod weights;

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use core::cmp::min;
pub use evm::{
	Config as EvmConfig, Context, ExitError, ExitFatal, ExitReason, ExitRevert, ExitSucceed,
};
use hash_db::Hasher;
use impl_trait_for_tuples::impl_for_tuples;
use scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
// Substrate
use frame_support::traits::tokens::WithdrawConsequence;
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, Pays, PostDispatchInfo},
	storage::{child::KillStorageResult, KeyPrefixIterator},
	traits::{
		fungible::{Balanced, Credit, Debt},
		tokens::{
			currency::Currency,
			fungible::Inspect,
			imbalance::{Imbalance, OnUnbalanced, SignedImbalance},
			ExistenceRequirement, Fortitude, Precision, Preservation, WithdrawReasons,
		},
		FindAuthor, Get, Time,
	},
	weights::Weight,
};
use frame_system::RawOrigin;
use sp_core::{H160, H256, U256};
use sp_runtime::{
	traits::{BadOrigin, NumberFor, Saturating, UniqueSaturatedInto, Zero},
	AccountId32, DispatchErrorWithPostInfo,
};
// Frontier
use fp_account::AccountId20;
use fp_evm::GenesisAccount;
pub use fp_evm::{
	Account, AccountProvider, CallInfo, CreateInfo, ExecutionInfoV2 as ExecutionInfo,
	FeeCalculator, IsPrecompileResult, LinearCostPrecompile, Log, Precompile, PrecompileFailure,
	PrecompileHandle, PrecompileOutput, PrecompileResult, PrecompileSet,
	TransactionValidationError, Vicinity,
};

pub use self::{
	pallet::*,
	runner::{Runner, RunnerError},
	weights::WeightInfo,
};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		/// Account info provider.
		#[pallet::no_default]
		type AccountProvider: AccountProvider;

		/// Calculator for current gas price.
		type FeeCalculator: FeeCalculator;

		/// Maps Ethereum gas to Substrate weight.
		type GasWeightMapping: GasWeightMapping;

		/// Weight corresponding to a gas unit.
		type WeightPerGas: Get<Weight>;

		/// Block number to block hash.
		#[pallet::no_default]
		type BlockHashMapping: BlockHashMapping;

		/// Allow the origin to call on behalf of given address.
		#[pallet::no_default_bounds]
		type CallOrigin: EnsureAddressOrigin<Self::RuntimeOrigin>;

		/// Allow the origin to withdraw on behalf of given address.
		#[pallet::no_default_bounds]
		type WithdrawOrigin: EnsureAddressOrigin<Self::RuntimeOrigin, Success = AccountIdOf<Self>>;

		/// Mapping from address to account id.
		#[pallet::no_default_bounds]
		type AddressMapping: AddressMapping<AccountIdOf<Self>>;

		/// Currency type for withdraw and balance storage.
		#[pallet::no_default]
		type Currency: Currency<AccountIdOf<Self>> + Inspect<AccountIdOf<Self>>;

		/// The overarching event type.
		#[pallet::no_default_bounds]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Precompiles associated with this EVM engine.
		type PrecompilesType: PrecompileSet;
		type PrecompilesValue: Get<Self::PrecompilesType>;

		/// Chain ID of EVM.
		type ChainId: Get<u64>;
		/// The block gas limit. Can be a simple constant, or an adjustment algorithm in another pallet.
		type BlockGasLimit: Get<U256>;

		/// EVM execution runner.
		#[pallet::no_default]
		type Runner: Runner<Self>;

		/// To handle fee deduction for EVM transactions. An example is this pallet being used by `pallet_ethereum`
		/// where the chain implementing `pallet_ethereum` should be able to configure what happens to the fees
		/// Similar to `OnChargeTransaction` of `pallet_transaction_payment`
		#[pallet::no_default_bounds]
		type OnChargeTransaction: OnChargeEVMTransaction<Self>;

		/// Called on create calls, used to record owner
		#[pallet::no_default_bounds]
		type OnCreate: OnCreate<Self>;

		/// Find author for the current block.
		type FindAuthor: FindAuthor<H160>;

		/// Gas limit Pov size ratio.
		type GasLimitPovSizeRatio: Get<u64>;

		/// Define the quick clear limit of storage clearing when a contract suicides. Set to 0 to disable it.
		type SuicideQuickClearLimit: Get<u32>;

		/// Get the timestamp for the current block.
		#[pallet::no_default]
		type Timestamp: Time;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// EVM config used in the module.
		fn config() -> &'static EvmConfig {
			&SHANGHAI_CONFIG
		}
	}

	pub mod config_preludes {
		use super::*;
		use core::str::FromStr;
		use frame_support::{derive_impl, parameter_types, ConsensusEngineId};
		use sp_runtime::traits::BlakeTwo256;

		pub struct TestDefaultConfig;

		#[derive_impl(
			frame_system::config_preludes::SolochainDefaultConfig,
			no_aggregated_types
		)]
		impl frame_system::DefaultConfig for TestDefaultConfig {}

		const BLOCK_GAS_LIMIT: u64 = 150_000_000;
		const MAX_POV_SIZE: u64 = 5 * 1024 * 1024;

		parameter_types! {
			pub BlockGasLimit: U256 = U256::from(BLOCK_GAS_LIMIT);
			pub const ChainId: u64 = 42;
			pub const GasLimitPovSizeRatio: u64 = BLOCK_GAS_LIMIT.saturating_div(MAX_POV_SIZE);
			pub WeightPerGas: Weight = Weight::from_parts(20_000, 0);
			pub SuicideQuickClearLimit: u32 = 0;
		}

		#[register_default_impl(TestDefaultConfig)]
		impl DefaultConfig for TestDefaultConfig {
			type CallOrigin = EnsureAddressRoot<Self::AccountId>;
			type WithdrawOrigin = EnsureAddressNever<Self::AccountId>;
			type AddressMapping = HashedAddressMapping<BlakeTwo256>;
			type FeeCalculator = FixedGasPrice;
			type GasWeightMapping = FixedGasWeightMapping<Self>;
			type WeightPerGas = WeightPerGas;
			#[inject_runtime_type]
			type RuntimeEvent = ();
			type PrecompilesType = ();
			type PrecompilesValue = ();
			type ChainId = ChainId;
			type BlockGasLimit = BlockGasLimit;
			type OnChargeTransaction = ();
			type OnCreate = ();
			type FindAuthor = FindAuthorTruncated;
			type GasLimitPovSizeRatio = GasLimitPovSizeRatio;
			type SuicideQuickClearLimit = SuicideQuickClearLimit;
			type WeightInfo = ();
		}

		impl FixedGasWeightMappingAssociatedTypes for TestDefaultConfig {
			type WeightPerGas = <Self as DefaultConfig>::WeightPerGas;
			type BlockWeights = <Self as frame_system::DefaultConfig>::BlockWeights;
			type GasLimitPovSizeRatio = <Self as DefaultConfig>::GasLimitPovSizeRatio;
		}

		pub struct FixedGasPrice;
		impl FeeCalculator for FixedGasPrice {
			fn min_gas_price() -> (U256, Weight) {
				(1.into(), Weight::zero())
			}
		}

		pub struct FindAuthorTruncated;
		impl FindAuthor<H160> for FindAuthorTruncated {
			fn find_author<'a, I>(_digests: I) -> Option<H160>
			where
				I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
			{
				Some(H160::from_str("1234500000000000000000000000000000000000").unwrap())
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Withdraw balance from EVM into currency/balances pallet.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::withdraw())]
		pub fn withdraw(
			origin: OriginFor<T>,
			address: H160,
			value: BalanceOf<T>,
		) -> DispatchResult {
			let destination = T::WithdrawOrigin::ensure_address_origin(&address, origin)?;
			let address_account_id = T::AddressMapping::into_account_id(address);

			T::Currency::transfer(
				&address_account_id,
				&destination,
				value,
				ExistenceRequirement::AllowDeath,
			)?;

			Ok(())
		}

		/// Issue an EVM call operation. This is similar to a message call transaction in Ethereum.
		#[pallet::call_index(1)]
		#[pallet::weight({
			let without_base_extrinsic_weight = true;
			T::GasWeightMapping::gas_to_weight(*gas_limit, without_base_extrinsic_weight)
		})]
		pub fn call(
			origin: OriginFor<T>,
			source: H160,
			target: H160,
			input: Vec<u8>,
			value: U256,
			gas_limit: u64,
			max_fee_per_gas: U256,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			access_list: Vec<(H160, Vec<H256>)>,
		) -> DispatchResultWithPostInfo {
			T::CallOrigin::ensure_address_origin(&source, origin)?;

			let is_transactional = true;
			let validate = true;
			let info = match T::Runner::call(
				source,
				target,
				input,
				value,
				gas_limit,
				Some(max_fee_per_gas),
				max_priority_fee_per_gas,
				nonce,
				access_list,
				is_transactional,
				validate,
				None,
				None,
				T::config(),
			) {
				Ok(info) => info,
				Err(e) => {
					return Err(DispatchErrorWithPostInfo {
						post_info: PostDispatchInfo {
							actual_weight: Some(e.weight),
							pays_fee: Pays::Yes,
						},
						error: e.error.into(),
					})
				}
			};

			match info.exit_reason {
				ExitReason::Succeed(_) => {
					Pallet::<T>::deposit_event(Event::<T>::Executed { address: target });
				}
				_ => {
					Pallet::<T>::deposit_event(Event::<T>::ExecutedFailed { address: target });
				}
			};

			Ok(PostDispatchInfo {
				actual_weight: {
					let mut gas_to_weight = T::GasWeightMapping::gas_to_weight(
						info.used_gas.standard.unique_saturated_into(),
						true,
					);
					if let Some(weight_info) = info.weight_info {
						if let Some(proof_size_usage) = weight_info.proof_size_usage {
							*gas_to_weight.proof_size_mut() = proof_size_usage;
						}
					}
					Some(gas_to_weight)
				},
				pays_fee: Pays::No,
			})
		}

		/// Issue an EVM create operation. This is similar to a contract creation transaction in
		/// Ethereum.
		#[pallet::call_index(2)]
		#[pallet::weight({
			let without_base_extrinsic_weight = true;
			T::GasWeightMapping::gas_to_weight(*gas_limit, without_base_extrinsic_weight)
		})]
		pub fn create(
			origin: OriginFor<T>,
			source: H160,
			init: Vec<u8>,
			value: U256,
			gas_limit: u64,
			max_fee_per_gas: U256,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			access_list: Vec<(H160, Vec<H256>)>,
		) -> DispatchResultWithPostInfo {
			T::CallOrigin::ensure_address_origin(&source, origin)?;

			let is_transactional = true;
			let validate = true;
			let info = match T::Runner::create(
				source,
				init,
				value,
				gas_limit,
				Some(max_fee_per_gas),
				max_priority_fee_per_gas,
				nonce,
				access_list,
				is_transactional,
				validate,
				None,
				None,
				T::config(),
			) {
				Ok(info) => info,
				Err(e) => {
					return Err(DispatchErrorWithPostInfo {
						post_info: PostDispatchInfo {
							actual_weight: Some(e.weight),
							pays_fee: Pays::Yes,
						},
						error: e.error.into(),
					})
				}
			};

			match info {
				CreateInfo {
					exit_reason: ExitReason::Succeed(_),
					value: create_address,
					..
				} => {
					Pallet::<T>::deposit_event(Event::<T>::Created {
						address: create_address,
					});
				}
				CreateInfo {
					exit_reason: _,
					value: create_address,
					..
				} => {
					Pallet::<T>::deposit_event(Event::<T>::CreatedFailed {
						address: create_address,
					});
				}
			}

			Ok(PostDispatchInfo {
				actual_weight: {
					let mut gas_to_weight = T::GasWeightMapping::gas_to_weight(
						info.used_gas.standard.unique_saturated_into(),
						true,
					);
					if let Some(weight_info) = info.weight_info {
						if let Some(proof_size_usage) = weight_info.proof_size_usage {
							*gas_to_weight.proof_size_mut() = proof_size_usage;
						}
					}
					Some(gas_to_weight)
				},
				pays_fee: Pays::No,
			})
		}

		/// Issue an EVM create2 operation.
		#[pallet::call_index(3)]
		#[pallet::weight({
			let without_base_extrinsic_weight = true;
			T::GasWeightMapping::gas_to_weight(*gas_limit, without_base_extrinsic_weight)
		})]
		pub fn create2(
			origin: OriginFor<T>,
			source: H160,
			init: Vec<u8>,
			salt: H256,
			value: U256,
			gas_limit: u64,
			max_fee_per_gas: U256,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			access_list: Vec<(H160, Vec<H256>)>,
		) -> DispatchResultWithPostInfo {
			T::CallOrigin::ensure_address_origin(&source, origin)?;

			let is_transactional = true;
			let validate = true;
			let info = match T::Runner::create2(
				source,
				init,
				salt,
				value,
				gas_limit,
				Some(max_fee_per_gas),
				max_priority_fee_per_gas,
				nonce,
				access_list,
				is_transactional,
				validate,
				None,
				None,
				T::config(),
			) {
				Ok(info) => info,
				Err(e) => {
					return Err(DispatchErrorWithPostInfo {
						post_info: PostDispatchInfo {
							actual_weight: Some(e.weight),
							pays_fee: Pays::Yes,
						},
						error: e.error.into(),
					})
				}
			};

			match info {
				CreateInfo {
					exit_reason: ExitReason::Succeed(_),
					value: create_address,
					..
				} => {
					Pallet::<T>::deposit_event(Event::<T>::Created {
						address: create_address,
					});
				}
				CreateInfo {
					exit_reason: _,
					value: create_address,
					..
				} => {
					Pallet::<T>::deposit_event(Event::<T>::CreatedFailed {
						address: create_address,
					});
				}
			}

			Ok(PostDispatchInfo {
				actual_weight: {
					let mut gas_to_weight = T::GasWeightMapping::gas_to_weight(
						info.used_gas.standard.unique_saturated_into(),
						true,
					);
					if let Some(weight_info) = info.weight_info {
						if let Some(proof_size_usage) = weight_info.proof_size_usage {
							*gas_to_weight.proof_size_mut() = proof_size_usage;
						}
					}
					Some(gas_to_weight)
				},
				pays_fee: Pays::No,
			})
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Ethereum events from contracts.
		Log { log: Log },
		/// A contract has been created at given address.
		Created { address: H160 },
		/// A contract was attempted to be created, but the execution failed.
		CreatedFailed { address: H160 },
		/// A contract has been executed successfully with states applied.
		Executed { address: H160 },
		/// A contract has been executed with errors. States are reverted with only gas fees applied.
		ExecutedFailed { address: H160 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough balance to perform action
		BalanceLow,
		/// Calculating total fee overflowed
		FeeOverflow,
		/// Calculating total payment overflowed
		PaymentOverflow,
		/// Withdraw fee failed
		WithdrawFailed,
		/// Gas price is too low.
		GasPriceTooLow,
		/// Nonce is invalid
		InvalidNonce,
		/// Gas limit is too low.
		GasLimitTooLow,
		/// Gas limit is too high.
		GasLimitTooHigh,
		/// The chain id is invalid.
		InvalidChainId,
		/// the signature is invalid.
		InvalidSignature,
		/// EVM reentrancy
		Reentrancy,
		/// EIP-3607,
		TransactionMustComeFromEOA,
		/// Undefined error.
		Undefined,
	}

	impl<T> From<TransactionValidationError> for Error<T> {
		fn from(validation_error: TransactionValidationError) -> Self {
			match validation_error {
				TransactionValidationError::GasLimitTooLow => Error::<T>::GasLimitTooLow,
				TransactionValidationError::GasLimitTooHigh => Error::<T>::GasLimitTooHigh,
				TransactionValidationError::BalanceTooLow => Error::<T>::BalanceLow,
				TransactionValidationError::TxNonceTooLow => Error::<T>::InvalidNonce,
				TransactionValidationError::TxNonceTooHigh => Error::<T>::InvalidNonce,
				TransactionValidationError::GasPriceTooLow => Error::<T>::GasPriceTooLow,
				TransactionValidationError::PriorityFeeTooHigh => Error::<T>::GasPriceTooLow,
				TransactionValidationError::InvalidFeeInput => Error::<T>::GasPriceTooLow,
				TransactionValidationError::InvalidChainId => Error::<T>::InvalidChainId,
				TransactionValidationError::InvalidSignature => Error::<T>::InvalidSignature,
				TransactionValidationError::UnknownError => Error::<T>::Undefined,
			}
		}
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub accounts: BTreeMap<H160, GenesisAccount>,
		#[serde(skip)]
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T>
	where
		U256: UniqueSaturatedInto<BalanceOf<T>>,
	{
		fn build(&self) {
			const MAX_ACCOUNT_NONCE: usize = 100;

			for (address, account) in &self.accounts {
				let account_id = T::AddressMapping::into_account_id(*address);

				// ASSUME: in one single EVM transaction, the nonce will not increase more than
				// `u128::max_value()`.
				for _ in 0..min(
					MAX_ACCOUNT_NONCE,
					UniqueSaturatedInto::<usize>::unique_saturated_into(account.nonce),
				) {
					T::AccountProvider::inc_account_nonce(&account_id);
				}

				let _ = T::Currency::deposit_creating(
					&account_id,
					account.balance.unique_saturated_into(),
				);

				Pallet::<T>::create_account(*address, account.code.clone());

				for (index, value) in &account.storage {
					<AccountStorages<T>>::insert(address, index, value);
				}
			}
		}
	}

	#[pallet::storage]
	pub type AccountCodes<T: Config> = StorageMap<_, Blake2_128Concat, H160, Vec<u8>, ValueQuery>;

	#[pallet::storage]
	pub type AccountCodesMetadata<T: Config> =
		StorageMap<_, Blake2_128Concat, H160, CodeMetadata, OptionQuery>;

	#[pallet::storage]
	pub type AccountStorages<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, H160, Blake2_128Concat, H256, H256, ValueQuery>;

	#[pallet::storage]
	pub type Suicided<T: Config> = StorageMap<_, Blake2_128Concat, H160, (), OptionQuery>;
}

/// Utility alias for easy access to the [`AccountProvider::AccountId`] type from a given config.
pub type AccountIdOf<T> = <<T as Config>::AccountProvider as AccountProvider>::AccountId;

/// Type alias for currency balance.
pub type BalanceOf<T> = <<T as Config>::Currency as Currency<AccountIdOf<T>>>::Balance;

/// Type alias for negative imbalance during fees
type NegativeImbalanceOf<C, T> = <C as Currency<AccountIdOf<T>>>::NegativeImbalance;

#[derive(
	Debug,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen
)]
pub struct CodeMetadata {
	pub size: u64,
	pub hash: H256,
}

impl CodeMetadata {
	fn from_code(code: &[u8]) -> Self {
		let size = code.len() as u64;
		let hash = H256::from(sp_io::hashing::keccak_256(code));

		Self { size, hash }
	}
}

pub trait EnsureAddressOrigin<OuterOrigin> {
	/// Success return type.
	type Success;

	/// Perform the origin check.
	fn ensure_address_origin(
		address: &H160,
		origin: OuterOrigin,
	) -> Result<Self::Success, BadOrigin> {
		Self::try_address_origin(address, origin).map_err(|_| BadOrigin)
	}

	/// Try with origin.
	fn try_address_origin(
		address: &H160,
		origin: OuterOrigin,
	) -> Result<Self::Success, OuterOrigin>;
}

/// Ensure that the EVM address is the same as the Substrate address. This only works if the account
/// ID is `H160`.
pub struct EnsureAddressSame;

impl<OuterOrigin> EnsureAddressOrigin<OuterOrigin> for EnsureAddressSame
where
	OuterOrigin: Into<Result<RawOrigin<H160>, OuterOrigin>> + From<RawOrigin<H160>>,
{
	type Success = H160;

	fn try_address_origin(address: &H160, origin: OuterOrigin) -> Result<H160, OuterOrigin> {
		origin.into().and_then(|o| match o {
			RawOrigin::Signed(who) if &who == address => Ok(who),
			r => Err(OuterOrigin::from(r)),
		})
	}
}

/// Ensure that the origin is root.
pub struct EnsureAddressRoot<AccountId>(core::marker::PhantomData<AccountId>);

impl<OuterOrigin, AccountId> EnsureAddressOrigin<OuterOrigin> for EnsureAddressRoot<AccountId>
where
	OuterOrigin: Into<Result<RawOrigin<AccountId>, OuterOrigin>> + From<RawOrigin<AccountId>>,
{
	type Success = ();

	fn try_address_origin(_address: &H160, origin: OuterOrigin) -> Result<(), OuterOrigin> {
		origin.into().and_then(|o| match o {
			RawOrigin::Root => Ok(()),
			r => Err(OuterOrigin::from(r)),
		})
	}
}

/// Ensure that the origin never happens.
pub struct EnsureAddressNever<AccountId>(core::marker::PhantomData<AccountId>);

impl<OuterOrigin, AccountId> EnsureAddressOrigin<OuterOrigin> for EnsureAddressNever<AccountId> {
	type Success = AccountId;

	fn try_address_origin(_address: &H160, origin: OuterOrigin) -> Result<AccountId, OuterOrigin> {
		Err(origin)
	}
}

/// Ensure that the address is truncated hash of the origin. Only works if the account id is
/// `AccountId32`.
pub struct EnsureAddressTruncated;

impl<OuterOrigin> EnsureAddressOrigin<OuterOrigin> for EnsureAddressTruncated
where
	OuterOrigin: Into<Result<RawOrigin<AccountId32>, OuterOrigin>> + From<RawOrigin<AccountId32>>,
{
	type Success = AccountId32;

	fn try_address_origin(address: &H160, origin: OuterOrigin) -> Result<AccountId32, OuterOrigin> {
		origin.into().and_then(|o| match o {
			RawOrigin::Signed(who) if AsRef::<[u8; 32]>::as_ref(&who)[0..20] == address[0..20] => {
				Ok(who)
			}
			r => Err(OuterOrigin::from(r)),
		})
	}
}

/// Ensure that the address is AccountId20.
pub struct EnsureAccountId20;

impl<OuterOrigin> EnsureAddressOrigin<OuterOrigin> for EnsureAccountId20
where
	OuterOrigin: Into<Result<RawOrigin<AccountId20>, OuterOrigin>> + From<RawOrigin<AccountId20>>,
{
	type Success = AccountId20;

	fn try_address_origin(address: &H160, origin: OuterOrigin) -> Result<AccountId20, OuterOrigin> {
		let acc: AccountId20 = AccountId20::from(*address);
		origin.into().and_then(|o| match o {
			RawOrigin::Signed(who) if who == acc => Ok(who),
			r => Err(OuterOrigin::from(r)),
		})
	}
}

/// Trait to be implemented for evm address mapping.
pub trait AddressMapping<A> {
	fn into_account_id(address: H160) -> A;
}

/// Identity address mapping.
pub struct IdentityAddressMapping;

impl<T: From<H160>> AddressMapping<T> for IdentityAddressMapping {
	fn into_account_id(address: H160) -> T {
		address.into()
	}
}

/// Hashed address mapping.
pub struct HashedAddressMapping<H>(core::marker::PhantomData<H>);

impl<H: Hasher<Out = H256>> AddressMapping<AccountId32> for HashedAddressMapping<H> {
	fn into_account_id(address: H160) -> AccountId32 {
		let mut data = [0u8; 24];
		data[0..4].copy_from_slice(b"evm:");
		data[4..24].copy_from_slice(&address[..]);
		let hash = H::hash(&data);

		AccountId32::from(Into::<[u8; 32]>::into(hash))
	}
}

/// A trait for getting a block hash by number.
pub trait BlockHashMapping {
	fn block_hash(number: u32) -> H256;
}

/// Returns the Substrate block hash by number.
pub struct SubstrateBlockHashMapping<T>(core::marker::PhantomData<T>);
impl<T: Config> BlockHashMapping for SubstrateBlockHashMapping<T> {
	fn block_hash(number: u32) -> H256 {
		let number = <NumberFor<T::Block>>::from(number);
		H256::from_slice(frame_system::Pallet::<T>::block_hash(number).as_ref())
	}
}

/// A mapping function that converts Ethereum gas to Substrate weight
pub trait GasWeightMapping {
	fn gas_to_weight(gas: u64, without_base_weight: bool) -> Weight;
	fn weight_to_gas(weight: Weight) -> u64;
}

pub trait FixedGasWeightMappingAssociatedTypes {
	type WeightPerGas: Get<Weight>;
	type BlockWeights: Get<frame_system::limits::BlockWeights>;
	type GasLimitPovSizeRatio: Get<u64>;
}

impl<T: Config> FixedGasWeightMappingAssociatedTypes for T {
	type WeightPerGas = T::WeightPerGas;
	type BlockWeights = T::BlockWeights;
	type GasLimitPovSizeRatio = T::GasLimitPovSizeRatio;
}

pub struct FixedGasWeightMapping<T>(core::marker::PhantomData<T>);
impl<T> GasWeightMapping for FixedGasWeightMapping<T>
where
	T: FixedGasWeightMappingAssociatedTypes,
{
	fn gas_to_weight(gas: u64, without_base_weight: bool) -> Weight {
		let mut weight = T::WeightPerGas::get().saturating_mul(gas);
		if without_base_weight {
			weight = weight.saturating_sub(
				T::BlockWeights::get()
					.get(frame_support::dispatch::DispatchClass::Normal)
					.base_extrinsic,
			);
		}
		// Apply a gas to proof size ratio based on BlockGasLimit
		let ratio = T::GasLimitPovSizeRatio::get();
		if ratio > 0 {
			let proof_size = gas.saturating_div(ratio);
			*weight.proof_size_mut() = proof_size;
		}

		weight
	}
	fn weight_to_gas(weight: Weight) -> u64 {
		weight.div(T::WeightPerGas::get().ref_time()).ref_time()
	}
}

static SHANGHAI_CONFIG: EvmConfig = EvmConfig::shanghai();

impl<T: Config> Pallet<T> {
	/// Check whether an account is empty.
	pub fn is_account_empty(address: &H160) -> bool {
		let (account, _) = Self::account_basic(address);
		let code_len = <AccountCodes<T>>::decode_len(address).unwrap_or(0);

		account.nonce == U256::zero() && account.balance == U256::zero() && code_len == 0
	}
	/// Check whether an account is a suicided contract
	pub fn is_account_suicided(address: &H160) -> bool {
		<Suicided<T>>::contains_key(address)
	}

	pub fn iter_account_storages(address: &H160) -> KeyPrefixIterator<H256> {
		<AccountStorages<T>>::iter_key_prefix(address)
	}

	/// Remove an account if its empty.
	pub fn remove_account_if_empty(address: &H160) {
		if Self::is_account_empty(address) {
			Self::remove_account(address);
		}
	}

	/// Remove an account.
	pub fn remove_account(address: &H160) {
		if <AccountCodes<T>>::contains_key(address) {
			// Remember to call `dec_sufficients` when clearing Suicided.
			<Suicided<T>>::insert(address, ());

			// In theory, we can always have pre-EIP161 contracts, so we
			// make sure the account nonce is at least one.
			let account_id = T::AddressMapping::into_account_id(*address);
			T::AccountProvider::inc_account_nonce(&account_id);
		}

		<AccountCodes<T>>::remove(address);
		<AccountCodesMetadata<T>>::remove(address);

		if T::SuicideQuickClearLimit::get() > 0 {
			#[allow(deprecated)]
			let res = <AccountStorages<T>>::remove_prefix(address, Some(T::SuicideQuickClearLimit::get()));

			match res {
				KillStorageResult::AllRemoved(_) => {
					<Suicided<T>>::remove(address);

					let account_id = T::AddressMapping::into_account_id(*address);
					T::AccountProvider::remove_account(&account_id);
				}
				KillStorageResult::SomeRemaining(_) => (),
			}
		}
	}

	/// Create an account.
	pub fn create_account(address: H160, code: Vec<u8>) {
		if <Suicided<T>>::contains_key(address) {
			// This branch should never trigger, because when Suicided
			// contains an address, then its nonce will be at least one,
			// which causes CreateCollision error in EVM, but we add it
			// here for safeguard.
			return;
		}

		if code.is_empty() {
			return;
		}

		if !<AccountCodes<T>>::contains_key(address) {
			let account_id = T::AddressMapping::into_account_id(address);
			T::AccountProvider::create_account(&account_id);
		}

		// Update metadata.
		let meta = CodeMetadata::from_code(&code);
		<AccountCodesMetadata<T>>::insert(address, meta);

		<AccountCodes<T>>::insert(address, code);
	}

	/// Get the account metadata (hash and size) from storage if it exists,
	/// or compute it from code and store it if it doesn't exist.
	pub fn account_code_metadata(address: H160) -> CodeMetadata {
		if let Some(meta) = <AccountCodesMetadata<T>>::get(address) {
			return meta;
		}

		let code = <AccountCodes<T>>::get(address);

		// If code is empty we return precomputed hash for empty code.
		// We don't store it as this address could get code deployed in the future.
		if code.is_empty() {
			const EMPTY_CODE_HASH: [u8; 32] = hex_literal::hex!(
				"c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
			);
			return CodeMetadata {
				size: 0,
				hash: EMPTY_CODE_HASH.into(),
			};
		}

		let meta = CodeMetadata::from_code(&code);

		<AccountCodesMetadata<T>>::insert(address, meta);
		meta
	}

	/// Get the account basic in EVM format.
	pub fn account_basic(address: &H160) -> (Account, frame_support::weights::Weight) {
		let account_id = T::AddressMapping::into_account_id(*address);
		let nonce = T::AccountProvider::account_nonce(&account_id);
		let balance =
			T::Currency::reducible_balance(&account_id, Preservation::Preserve, Fortitude::Polite);

		(
			Account {
				nonce: U256::from(UniqueSaturatedInto::<u128>::unique_saturated_into(nonce)),
				balance: U256::from(UniqueSaturatedInto::<u128>::unique_saturated_into(balance)),
			},
			T::DbWeight::get().reads(2),
		)
	}

	/// Get the author using the FindAuthor trait.
	pub fn find_author() -> H160 {
		let digest = <frame_system::Pallet<T>>::digest();
		let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());

		T::FindAuthor::find_author(pre_runtime_digests).unwrap_or_default()
	}
}

/// Handle withdrawing, refunding and depositing of transaction fees.
/// Similar to `OnChargeTransaction` of `pallet_transaction_payment`
pub trait OnChargeEVMTransaction<T: Config> {
	type LiquidityInfo: Default;

	/// Before the transaction is executed the payment of the transaction fees
	/// need to be secured.
	fn withdraw_fee(who: &H160, fee: U256) -> Result<Self::LiquidityInfo, Error<T>>;

	fn can_withdraw(who: &H160, amount: U256) -> Result<(), Error<T>>;

	/// After the transaction was executed the actual fee can be calculated.
	/// This function should refund any overpaid fees and optionally deposit
	/// the corrected amount, and handles the base fee rationing using the provided
	/// `OnUnbalanced` implementation.
	/// Returns the `NegativeImbalance` - if any - produced by the priority fee.
	fn correct_and_deposit_fee(
		who: &H160,
		corrected_fee: U256,
		base_fee: U256,
		already_withdrawn: Self::LiquidityInfo,
	) -> Self::LiquidityInfo;

	/// Introduced in EIP1559 to handle the priority tip.
	fn pay_priority_fee(tip: Self::LiquidityInfo);
}

/// Implements the transaction payment for a pallet implementing the `Currency`
/// trait (eg. the pallet_balances) using an unbalance handler (implementing
/// `OnUnbalanced`).
/// Similar to `CurrencyAdapter` of `pallet_transaction_payment`
pub struct EVMCurrencyAdapter<C, OU>(core::marker::PhantomData<(C, OU)>);

impl<T, C, OU> OnChargeEVMTransaction<T> for EVMCurrencyAdapter<C, OU>
where
	T: Config,
	C: Currency<AccountIdOf<T>>,
	C::PositiveImbalance:
		Imbalance<<C as Currency<AccountIdOf<T>>>::Balance, Opposite = C::NegativeImbalance>,
	C::NegativeImbalance:
		Imbalance<<C as Currency<AccountIdOf<T>>>::Balance, Opposite = C::PositiveImbalance>,
	OU: OnUnbalanced<NegativeImbalanceOf<C, T>>,
	U256: UniqueSaturatedInto<<C as Currency<AccountIdOf<T>>>::Balance>,
{
	// Kept type as Option to satisfy bound of Default
	type LiquidityInfo = Option<NegativeImbalanceOf<C, T>>;

	fn withdraw_fee(who: &H160, fee: U256) -> Result<Self::LiquidityInfo, Error<T>> {
		if fee.is_zero() {
			return Ok(None);
		}
		let account_id = T::AddressMapping::into_account_id(*who);
		let imbalance = C::withdraw(
			&account_id,
			fee.unique_saturated_into(),
			WithdrawReasons::FEE,
			ExistenceRequirement::AllowDeath,
		)
		.map_err(|_| Error::<T>::BalanceLow)?;
		Ok(Some(imbalance))
	}

	fn can_withdraw(who: &H160, amount: U256) -> Result<(), Error<T>> {
		let account_id = T::AddressMapping::into_account_id(*who);
		let amount = amount.unique_saturated_into();
		let new_free = C::free_balance(&account_id).saturating_sub(amount);
		C::ensure_can_withdraw(
			&account_id,
			amount,
			WithdrawReasons::FEE, // note that this is ignored in ensure_can_withdraw()
			new_free,
		)
		.map_err(|_| Error::<T>::BalanceLow)?;
		Ok(())
	}

	fn correct_and_deposit_fee(
		who: &H160,
		corrected_fee: U256,
		base_fee: U256,
		already_withdrawn: Self::LiquidityInfo,
	) -> Self::LiquidityInfo {
		if let Some(paid) = already_withdrawn {
			let account_id = T::AddressMapping::into_account_id(*who);

			// Calculate how much refund we should return
			let refund_amount = paid
				.peek()
				.saturating_sub(corrected_fee.unique_saturated_into());
			// refund to the account that paid the fees. If this fails, the
			// account might have dropped below the existential balance. In
			// that case we don't refund anything.
			let refund_imbalance = C::deposit_into_existing(&account_id, refund_amount)
				.unwrap_or_else(|_| C::PositiveImbalance::zero());

			// Make sure this works with 0 ExistentialDeposit
			// https://github.com/paritytech/substrate/issues/10117
			// If we tried to refund something, the account still empty and the ED is set to 0,
			// we call `make_free_balance_be` with the refunded amount.
			let refund_imbalance = if C::minimum_balance().is_zero()
				&& refund_amount > C::Balance::zero()
				&& C::total_balance(&account_id).is_zero()
			{
				// Known bug: Substrate tried to refund to a zeroed AccountData, but
				// interpreted the account to not exist.
				match C::make_free_balance_be(&account_id, refund_amount) {
					SignedImbalance::Positive(p) => p,
					_ => C::PositiveImbalance::zero(),
				}
			} else {
				refund_imbalance
			};

			// merge the imbalance caused by paying the fees and refunding parts of it again.
			let adjusted_paid = paid
				.offset(refund_imbalance)
				.same()
				.unwrap_or_else(|_| C::NegativeImbalance::zero());

			let (base_fee, tip) = adjusted_paid.split(base_fee.unique_saturated_into());
			// Handle base fee. Can be either burned, rationed, etc ...
			OU::on_unbalanced(base_fee);
			return Some(tip);
		}
		None
	}

	fn pay_priority_fee(tip: Self::LiquidityInfo) {
		// Default Ethereum behaviour: issue the tip to the block author.
		if let Some(tip) = tip {
			let account_id = T::AddressMapping::into_account_id(<Pallet<T>>::find_author());
			let _ = C::deposit_into_existing(&account_id, tip.peek());
		}
	}
}
/// Implements transaction payment for a pallet implementing the [`fungible`]
/// trait (eg. pallet_balances) using an unbalance handler (implementing
/// [`OnUnbalanced`]).
///
/// Equivalent of `EVMCurrencyAdapter` but for fungible traits. Similar to `FungibleAdapter` of
/// `pallet_transaction_payment`
pub struct EVMFungibleAdapter<F, OU>(core::marker::PhantomData<(F, OU)>);

impl<T, F, OU> OnChargeEVMTransaction<T> for EVMFungibleAdapter<F, OU>
where
	T: Config,
	F: Balanced<AccountIdOf<T>>,
	OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
	U256: UniqueSaturatedInto<<F as Inspect<AccountIdOf<T>>>::Balance>,
{
	// Kept type as Option to satisfy bound of Default
	type LiquidityInfo = Option<Credit<AccountIdOf<T>, F>>;

	fn withdraw_fee(who: &H160, fee: U256) -> Result<Self::LiquidityInfo, Error<T>> {
		if fee.is_zero() {
			return Ok(None);
		}
		let account_id = T::AddressMapping::into_account_id(*who);
		let imbalance = F::withdraw(
			&account_id,
			fee.unique_saturated_into(),
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)
		.map_err(|_| Error::<T>::BalanceLow)?;
		Ok(Some(imbalance))
	}

	fn can_withdraw(who: &H160, amount: U256) -> Result<(), Error<T>> {
		let account_id = T::AddressMapping::into_account_id(*who);
		let amount = amount.unique_saturated_into();
		if let WithdrawConsequence::Success = F::can_withdraw(&account_id, amount) {
			return Ok(());
		}
		Err(Error::<T>::BalanceLow)
	}

	fn correct_and_deposit_fee(
		who: &H160,
		corrected_fee: U256,
		base_fee: U256,
		already_withdrawn: Self::LiquidityInfo,
	) -> Self::LiquidityInfo {
		if let Some(paid) = already_withdrawn {
			let account_id = T::AddressMapping::into_account_id(*who);

			// Calculate how much refund we should return
			let refund_amount = paid
				.peek()
				.saturating_sub(corrected_fee.unique_saturated_into());
			// refund to the account that paid the fees.
			let refund_imbalance = F::deposit(&account_id, refund_amount, Precision::BestEffort)
				.unwrap_or_else(|_| Debt::<AccountIdOf<T>, F>::zero());

			// merge the imbalance caused by paying the fees and refunding parts of it again.
			let adjusted_paid = paid
				.offset(refund_imbalance)
				.same()
				.unwrap_or_else(|_| Credit::<AccountIdOf<T>, F>::zero());

			let (base_fee, tip) = adjusted_paid.split(base_fee.unique_saturated_into());
			// Handle base fee. Can be either burned, rationed, etc ...
			OU::on_unbalanced(base_fee);
			return Some(tip);
		}
		None
	}

	fn pay_priority_fee(tip: Self::LiquidityInfo) {
		// Default Ethereum behaviour: issue the tip to the block author.
		if let Some(tip) = tip {
			let account_id = T::AddressMapping::into_account_id(<Pallet<T>>::find_author());
			let _ = F::deposit(&account_id, tip.peek(), Precision::BestEffort);
		}
	}
}

/// Implementation for () does not specify what to do with imbalance
impl<T> OnChargeEVMTransaction<T> for ()
where
	T: Config,
	T::Currency: Balanced<AccountIdOf<T>>,
	U256: UniqueSaturatedInto<<<T as Config>::Currency as Inspect<AccountIdOf<T>>>::Balance>,
{
	// Kept type as Option to satisfy bound of Default
	type LiquidityInfo = Option<Credit<AccountIdOf<T>, T::Currency>>;

	fn withdraw_fee(who: &H160, fee: U256) -> Result<Self::LiquidityInfo, Error<T>> {
		EVMFungibleAdapter::<T::Currency, ()>::withdraw_fee(who, fee)
	}

	fn correct_and_deposit_fee(
		who: &H160,
		corrected_fee: U256,
		base_fee: U256,
		already_withdrawn: Self::LiquidityInfo,
	) -> Self::LiquidityInfo {
		<EVMFungibleAdapter<T::Currency, ()> as OnChargeEVMTransaction<T>>::correct_and_deposit_fee(
			who,
			corrected_fee,
			base_fee,
			already_withdrawn,
		)
	}

	fn pay_priority_fee(tip: Self::LiquidityInfo) {
		<EVMFungibleAdapter<T::Currency, ()> as OnChargeEVMTransaction<T>>::pay_priority_fee(tip);
	}

	fn can_withdraw(who: &H160, amount: U256) -> Result<(), Error<T>> {
		EVMFungibleAdapter::<T::Currency, ()>::can_withdraw(who, amount)
	}
}

pub trait OnCreate<T> {
	fn on_create(owner: H160, contract: H160);
}

impl<T> OnCreate<T> for () {
	fn on_create(_owner: H160, _contract: H160) {}
}

#[impl_for_tuples(1, 12)]
impl<T> OnCreate<T> for Tuple {
	fn on_create(owner: H160, contract: H160) {
		for_tuples!(#(
			Tuple::on_create(owner, contract);
		)*)
	}
}

/// EVM account provider based on the [`frame_system`] accounts.
///
/// Uses standard Substrate accounts system to hold EVM accounts.
pub struct FrameSystemAccountProvider<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> AccountProvider for FrameSystemAccountProvider<T> {
	type AccountId = T::AccountId;
	type Nonce = T::Nonce;

	fn account_nonce(who: &Self::AccountId) -> Self::Nonce {
		frame_system::Pallet::<T>::account_nonce(who)
	}

	fn inc_account_nonce(who: &Self::AccountId) {
		frame_system::Pallet::<T>::inc_account_nonce(who)
	}

	fn create_account(who: &Self::AccountId) {
		let _ = frame_system::Pallet::<T>::inc_sufficients(who);
	}

	fn remove_account(who: &Self::AccountId) {
		let _ = frame_system::Pallet::<T>::dec_sufficients(who);
	}
}
