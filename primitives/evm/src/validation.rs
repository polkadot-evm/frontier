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

#![allow(clippy::comparison_chain)]

use alloc::vec::Vec;
pub use evm::backend::Basic as Account;
use frame_support::{sp_runtime::traits::UniqueSaturatedInto, weights::Weight};
use sp_core::{H160, H256, U256};

#[derive(Debug)]
pub struct CheckEvmTransactionInput {
	pub chain_id: Option<u64>,
	pub to: Option<H160>,
	pub input: Vec<u8>,
	pub nonce: U256,
	pub gas_limit: U256,
	pub gas_price: Option<U256>,
	pub max_fee_per_gas: Option<U256>,
	pub max_priority_fee_per_gas: Option<U256>,
	pub value: U256,
	pub access_list: Vec<(H160, Vec<H256>)>,
}

#[derive(Debug)]
pub struct CheckEvmTransactionConfig<'config> {
	pub evm_config: &'config evm::Config,
	pub block_gas_limit: U256,
	pub base_fee: U256,
	pub chain_id: u64,
	pub is_transactional: bool,
}

#[derive(Debug)]
pub struct CheckEvmTransaction<'config, E: From<TransactionValidationError>> {
	pub config: CheckEvmTransactionConfig<'config>,
	pub transaction: CheckEvmTransactionInput,
	pub weight_limit: Option<Weight>,
	pub proof_size_base_cost: Option<u64>,
	_marker: core::marker::PhantomData<E>,
}

/// Transaction validation errors
#[repr(u8)]
#[derive(num_enum::FromPrimitive, num_enum::IntoPrimitive, Debug)]
pub enum TransactionValidationError {
	/// The transaction gas limit is too low
	GasLimitTooLow,
	/// The transaction gas limit is too hign
	GasLimitTooHigh,
	/// The transaction gas price is too low
	GasPriceTooLow,
	/// The transaction priority fee is too high
	PriorityFeeTooHigh,
	/// The transaction balance is too low
	BalanceTooLow,
	/// The transaction nonce is too low
	TxNonceTooLow,
	/// The transaction nonce is too high
	TxNonceTooHigh,
	/// The transaction fee input is invalid
	InvalidFeeInput,
	/// The chain id is incorrect
	InvalidChainId,
	/// The transaction signature is invalid
	InvalidSignature,
	/// Unknown error
	#[num_enum(default)]
	UnknownError,
}

impl<'config, E: From<TransactionValidationError>> CheckEvmTransaction<'config, E> {
	pub fn new(
		config: CheckEvmTransactionConfig<'config>,
		transaction: CheckEvmTransactionInput,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
	) -> Self {
		CheckEvmTransaction {
			config,
			transaction,
			weight_limit,
			proof_size_base_cost,
			_marker: Default::default(),
		}
	}

	pub fn validate_in_pool_for(&self, who: &Account) -> Result<&Self, E> {
		if self.transaction.nonce < who.nonce {
			return Err(TransactionValidationError::TxNonceTooLow.into());
		}
		self.validate_common()
	}

	pub fn validate_in_block_for(&self, who: &Account) -> Result<&Self, E> {
		if self.transaction.nonce > who.nonce {
			return Err(TransactionValidationError::TxNonceTooHigh.into());
		} else if self.transaction.nonce < who.nonce {
			return Err(TransactionValidationError::TxNonceTooLow.into());
		}
		self.validate_common()
	}

	pub fn with_chain_id(&self) -> Result<&Self, E> {
		// Chain id matches the one in the signature.
		if let Some(chain_id) = self.transaction.chain_id {
			if chain_id != self.config.chain_id {
				return Err(TransactionValidationError::InvalidChainId.into());
			}
		}
		Ok(self)
	}

	pub fn with_base_fee(&self) -> Result<&Self, E> {
		// Get fee data from either a legacy or typed transaction input.
		let (gas_price, _) = self.transaction_fee_input()?;
		if self.config.is_transactional || gas_price > U256::zero() {
			// Transaction max fee is at least the current base fee.
			if gas_price < self.config.base_fee {
				return Err(TransactionValidationError::GasPriceTooLow.into());
			}
		}
		Ok(self)
	}

	pub fn with_balance_for(&self, who: &Account) -> Result<&Self, E> {
		// Get fee data from either a legacy or typed transaction input.
		let (max_fee_per_gas, _) = self.transaction_fee_input()?;

		// Account has enough funds to pay for the transaction.
		// Check is skipped on non-transactional calls that don't provide
		// a gas price input.
		//
		// Validation for EIP-1559 is done using the max_fee_per_gas, which is
		// the most a txn could possibly pay.
		//
		// Fee for Legacy or EIP-2930 transaction is calculated using
		// the provided `gas_price`.
		let fee = max_fee_per_gas.saturating_mul(self.transaction.gas_limit);
		if self.config.is_transactional || fee > U256::zero() {
			let total_payment = self.transaction.value.saturating_add(fee);
			if who.balance < total_payment {
				return Err(TransactionValidationError::BalanceTooLow.into());
			}
		}
		Ok(self)
	}

	// Returns the max_fee_per_gas (or gas_price for legacy txns) as well as an optional
	// effective_gas_price for EIP-1559 transactions. effective_gas_price represents
	// the total (fee + tip) that would be paid given the current base_fee.
	fn transaction_fee_input(&self) -> Result<(U256, Option<U256>), E> {
		match (
			self.transaction.gas_price,
			self.transaction.max_fee_per_gas,
			self.transaction.max_priority_fee_per_gas,
		) {
			// Legacy or EIP-2930 transaction.
			(Some(gas_price), None, None) => Ok((gas_price, Some(gas_price))),
			// EIP-1559 transaction without tip.
			(None, Some(max_fee_per_gas), None) => {
				Ok((max_fee_per_gas, Some(self.config.base_fee)))
			}
			// EIP-1559 tip.
			(None, Some(max_fee_per_gas), Some(max_priority_fee_per_gas)) => {
				if max_priority_fee_per_gas > max_fee_per_gas {
					return Err(TransactionValidationError::PriorityFeeTooHigh.into());
				}
				let effective_gas_price = self
					.config
					.base_fee
					.checked_add(max_priority_fee_per_gas)
					.unwrap_or_else(U256::max_value)
					.min(max_fee_per_gas);
				Ok((max_fee_per_gas, Some(effective_gas_price)))
			}
			_ => {
				if self.config.is_transactional {
					Err(TransactionValidationError::InvalidFeeInput.into())
				} else {
					// Allow non-set fee input for non-transactional calls.
					Ok((U256::zero(), None))
				}
			}
		}
	}

	pub fn max_withdraw_amount(&self) -> Result<U256, E> {
		Ok(self
			.transaction_fee_input()?
			.0 // max_fee_per_gas
			.saturating_mul(self.transaction.gas_limit)
			.saturating_add(self.transaction.value))
	}

	pub fn validate_common(&self) -> Result<&Self, E> {
		if self.config.is_transactional {
			// Try to subtract the proof_size_base_cost from the Weight proof_size limit or fail.
			// Validate the weight limit can afford recording the proof size cost.
			if let (Some(weight_limit), Some(proof_size_base_cost)) =
				(self.weight_limit, self.proof_size_base_cost)
			{
				let _ = weight_limit
					.proof_size()
					.checked_sub(proof_size_base_cost)
					.ok_or(TransactionValidationError::GasLimitTooLow)?;
			}

			// We must ensure a transaction can pay the cost of its data bytes.
			// If it can't it should not be included in a block.
			let mut gasometer = evm::gasometer::Gasometer::new(
				self.transaction.gas_limit.unique_saturated_into(),
				self.config.evm_config,
			);
			let transaction_cost = if self.transaction.to.is_some() {
				evm::gasometer::call_transaction_cost(
					&self.transaction.input,
					&self.transaction.access_list,
				)
			} else {
				evm::gasometer::create_transaction_cost(
					&self.transaction.input,
					&self.transaction.access_list,
				)
			};

			if gasometer.record_transaction(transaction_cost).is_err() {
				return Err(TransactionValidationError::GasLimitTooLow.into());
			}

			// Transaction gas limit is within the upper bound block gas limit.
			if self.transaction.gas_limit > self.config.block_gas_limit {
				return Err(TransactionValidationError::GasLimitTooHigh.into());
			}
		}

		Ok(self)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[derive(Debug, PartialEq)]
	pub enum TestError {
		GasLimitTooLow,
		GasLimitTooHigh,
		GasPriceTooLow,
		PriorityFeeTooHigh,
		BalanceTooLow,
		TxNonceTooLow,
		TxNonceTooHigh,
		InvalidFeeInput,
		InvalidChainId,
		InvalidSignature,
		UnknownError,
	}

	static CANCUN_CONFIG: evm::Config = evm::Config::cancun();

	impl From<TransactionValidationError> for TestError {
		fn from(e: TransactionValidationError) -> Self {
			match e {
				TransactionValidationError::GasLimitTooLow => TestError::GasLimitTooLow,
				TransactionValidationError::GasLimitTooHigh => TestError::GasLimitTooHigh,
				TransactionValidationError::GasPriceTooLow => TestError::GasPriceTooLow,
				TransactionValidationError::PriorityFeeTooHigh => TestError::PriorityFeeTooHigh,
				TransactionValidationError::BalanceTooLow => TestError::BalanceTooLow,
				TransactionValidationError::TxNonceTooLow => TestError::TxNonceTooLow,
				TransactionValidationError::TxNonceTooHigh => TestError::TxNonceTooHigh,
				TransactionValidationError::InvalidFeeInput => TestError::InvalidFeeInput,
				TransactionValidationError::InvalidChainId => TestError::InvalidChainId,
				TransactionValidationError::InvalidSignature => TestError::InvalidSignature,
				TransactionValidationError::UnknownError => TestError::UnknownError,
			}
		}
	}

	struct TestCase {
		pub blockchain_gas_limit: U256,
		pub blockchain_base_fee: U256,
		pub blockchain_chain_id: u64,
		pub is_transactional: bool,
		pub chain_id: Option<u64>,
		pub nonce: U256,
		pub gas_limit: U256,
		pub gas_price: Option<U256>,
		pub max_fee_per_gas: Option<U256>,
		pub max_priority_fee_per_gas: Option<U256>,
		pub value: U256,
		pub weight_limit: Option<Weight>,
		pub proof_size_base_cost: Option<u64>,
	}

	impl Default for TestCase {
		fn default() -> Self {
			TestCase {
				blockchain_gas_limit: U256::max_value(),
				blockchain_base_fee: U256::from(1_000_000_000u128),
				blockchain_chain_id: 42u64,
				is_transactional: true,
				chain_id: Some(42u64),
				nonce: U256::zero(),
				gas_limit: U256::from(21_000u64),
				gas_price: None,
				max_fee_per_gas: Some(U256::from(1_000_000_000u128)),
				max_priority_fee_per_gas: Some(U256::from(1_000_000_000u128)),
				value: U256::from(1u8),
				weight_limit: None,
				proof_size_base_cost: None,
			}
		}
	}

	fn test_env<'config>(input: TestCase) -> CheckEvmTransaction<'config, TestError> {
		let TestCase {
			blockchain_gas_limit,
			blockchain_base_fee,
			blockchain_chain_id,
			is_transactional,
			chain_id,
			nonce,
			gas_limit,
			gas_price,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			value,
			weight_limit,
			proof_size_base_cost,
		} = input;
		CheckEvmTransaction::<TestError>::new(
			CheckEvmTransactionConfig {
				evm_config: &CANCUN_CONFIG,
				block_gas_limit: blockchain_gas_limit,
				base_fee: blockchain_base_fee,
				chain_id: blockchain_chain_id,
				is_transactional,
			},
			CheckEvmTransactionInput {
				chain_id,
				to: Some(H160::default()),
				input: vec![],
				nonce,
				gas_limit,
				gas_price,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				value,
				access_list: vec![],
			},
			weight_limit,
			proof_size_base_cost,
		)
	}

	// Transaction settings
	fn default_transaction<'config>(
		is_transactional: bool,
	) -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			is_transactional,
			..Default::default()
		})
	}

	fn transaction_gas_limit_low<'config>(
		is_transactional: bool,
	) -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			gas_limit: U256::from(1u8),
			is_transactional,
			..Default::default()
		})
	}

	fn transaction_gas_limit_low_proof_size<'config>(
		is_transactional: bool,
	) -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			weight_limit: Some(Weight::from_parts(1, 1)),
			proof_size_base_cost: Some(2),
			is_transactional,
			..Default::default()
		})
	}

	fn transaction_gas_limit_high<'config>() -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			blockchain_gas_limit: U256::from(1u8),
			..Default::default()
		})
	}

	fn transaction_nonce_high<'config>() -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			nonce: U256::from(10u8),
			..Default::default()
		})
	}

	fn transaction_invalid_chain_id<'config>() -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			chain_id: Some(555u64),
			..Default::default()
		})
	}

	fn transaction_none_fee<'config>(
		is_transactional: bool,
	) -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			max_fee_per_gas: None,
			max_priority_fee_per_gas: None,
			is_transactional,
			..Default::default()
		})
	}

	fn transaction_max_fee_low<'config>(
		is_transactional: bool,
	) -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			max_fee_per_gas: Some(U256::from(1u8)),
			max_priority_fee_per_gas: None,
			is_transactional,
			..Default::default()
		})
	}

	fn transaction_priority_fee_high<'config>(
		is_transactional: bool,
	) -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			max_priority_fee_per_gas: Some(U256::from(1_100_000_000)),
			is_transactional,
			..Default::default()
		})
	}

	fn transaction_max_fee_high<'config>(tip: bool) -> CheckEvmTransaction<'config, TestError> {
		let mut input = TestCase {
			max_fee_per_gas: Some(U256::from(5_000_000_000u128)),
			..Default::default()
		};
		if !tip {
			input.max_priority_fee_per_gas = None;
		}
		test_env(input)
	}

	fn legacy_transaction<'config>() -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			gas_price: Some(U256::from(1_000_000_000u128)),
			max_fee_per_gas: None,
			max_priority_fee_per_gas: None,
			..Default::default()
		})
	}

	fn invalid_transaction_mixed_fees<'config>(
		is_transactional: bool,
	) -> CheckEvmTransaction<'config, TestError> {
		test_env(TestCase {
			gas_price: Some(U256::from(1_000_000_000u128)),
			max_fee_per_gas: Some(U256::from(1_000_000_000u128)),
			max_priority_fee_per_gas: None,
			is_transactional,
			..Default::default()
		})
	}

	// Default (valid) transaction succeeds in pool and in block.
	#[test]
	fn validate_in_pool_and_block_succeeds() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::zero(),
		};
		let test = default_transaction(true);
		// Pool
		assert!(test.validate_in_pool_for(&who).is_ok());
		// Block
		assert!(test.validate_in_block_for(&who).is_ok());
	}

	// Nonce too low fails in pool and in block.
	#[test]
	fn validate_in_pool_and_block_fails_nonce_too_low() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::from(1u8),
		};
		let test = default_transaction(true);
		// Pool
		let res = test.validate_in_pool_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::TxNonceTooLow);
		// Block
		let res = test.validate_in_block_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::TxNonceTooLow);
	}

	// Nonce too high succeeds in pool.
	#[test]
	fn validate_in_pool_succeeds_nonce_too_high() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::from(1u8),
		};
		let test = transaction_nonce_high();
		let res = test.validate_in_pool_for(&who);
		assert!(res.is_ok());
	}

	// Nonce too high fails in block.
	#[test]
	fn validate_in_block_fails_nonce_too_high() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::from(1u8),
		};
		let test = transaction_nonce_high();
		let res = test.validate_in_block_for(&who);
		assert!(res.is_err());
	}

	// Gas limit too low transactional fails in pool and in block.
	#[test]
	fn validate_in_pool_and_block_transactional_fails_gas_limit_too_low() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::zero(),
		};
		let is_transactional = true;
		let test = transaction_gas_limit_low(is_transactional);
		// Pool
		let res = test.validate_in_pool_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasLimitTooLow);
		// Block
		let res = test.validate_in_block_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasLimitTooLow);
	}

	// Gas limit too low non-transactional succeeds in pool and in block.
	#[test]
	fn validate_in_pool_and_block_non_transactional_succeeds_gas_limit_too_low() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::zero(),
		};
		let is_transactional = false;
		let test = transaction_gas_limit_low(is_transactional);
		// Pool
		let res = test.validate_in_pool_for(&who);
		assert!(res.is_ok());
		// Block
		let res = test.validate_in_block_for(&who);
		assert!(res.is_ok());
	}

	// Gas limit too low for proof size recording transactional fails in pool and in block.
	#[test]
	fn validate_in_pool_and_block_transactional_fails_gas_limit_too_low_proof_size() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::zero(),
		};
		let is_transactional = true;
		let test = transaction_gas_limit_low_proof_size(is_transactional);
		// Pool
		let res = test.validate_in_pool_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasLimitTooLow);
		// Block
		let res = test.validate_in_block_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasLimitTooLow);
	}

	// Gas limit too low non-transactional succeeds in pool and in block.
	#[test]
	fn validate_in_pool_and_block_non_transactional_succeeds_gas_limit_too_low_proof_size() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::zero(),
		};
		let is_transactional = false;
		let test = transaction_gas_limit_low_proof_size(is_transactional);
		// Pool
		let res = test.validate_in_pool_for(&who);
		assert!(res.is_ok());
		// Block
		let res = test.validate_in_block_for(&who);
		assert!(res.is_ok());
	}

	// Gas limit too high fails in pool and in block.
	#[test]
	fn validate_in_pool_for_fails_gas_limit_too_high() {
		let who = Account {
			balance: U256::from(1_000_000u128),
			nonce: U256::zero(),
		};
		let test = transaction_gas_limit_high();
		// Pool
		let res = test.validate_in_pool_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasLimitTooHigh);
		// Block
		let res = test.validate_in_block_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasLimitTooHigh);
	}

	// Valid chain id succeeds.
	#[test]
	fn validate_chain_id_succeeds() {
		let test = default_transaction(true);
		let res = test.with_chain_id();
		assert!(res.is_ok());
	}

	// Invalid chain id fails.
	#[test]
	fn validate_chain_id_fails() {
		let test = transaction_invalid_chain_id();
		let res = test.with_chain_id();
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::InvalidChainId);
	}

	// Valid max fee per gas succeeds.
	#[test]
	fn validate_base_fee_succeeds() {
		// Transactional
		let test = default_transaction(true);
		let res = test.with_base_fee();
		assert!(res.is_ok());
		// Non-transactional
		let test = default_transaction(false);
		let res = test.with_base_fee();
		assert!(res.is_ok());
	}

	// Transactional call with unset fee data fails.
	#[test]
	fn validate_base_fee_with_none_fee_fails() {
		let test = transaction_none_fee(true);
		let res = test.with_base_fee();
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::InvalidFeeInput);
	}

	// Non-transactional call with unset fee data succeeds.
	#[test]
	fn validate_base_fee_with_none_fee_non_transactional_succeeds() {
		let test = transaction_none_fee(false);
		let res = test.with_base_fee();
		assert!(res.is_ok());
	}

	// Max fee per gas too low fails.
	#[test]
	fn validate_base_fee_with_max_fee_too_low_fails() {
		// Transactional
		let test = transaction_max_fee_low(true);
		let res = test.with_base_fee();
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasPriceTooLow);
		// Non-transactional
		let test = transaction_max_fee_low(false);
		let res = test.with_base_fee();
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::GasPriceTooLow);
	}

	// Priority fee too high fails.
	#[test]
	fn validate_base_fee_with_priority_fee_too_high_fails() {
		// Transactional
		let test = transaction_priority_fee_high(true);
		let res = test.with_base_fee();
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::PriorityFeeTooHigh);
		// Non-transactional
		let test = transaction_priority_fee_high(false);
		let res = test.with_base_fee();
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::PriorityFeeTooHigh);
	}

	// Sufficient balance succeeds.
	#[test]
	fn validate_balance_succeeds() {
		let who = Account {
			balance: U256::from(21_000_000_000_001u128),
			nonce: U256::zero(),
		};
		// Transactional
		let test = default_transaction(true);
		let res = test.with_balance_for(&who);
		assert!(res.is_ok());
		// Non-transactional
		let test = default_transaction(false);
		let res = test.with_balance_for(&who);
		assert!(res.is_ok());
	}

	// Insufficient balance fails.
	#[test]
	fn validate_insufficient_balance_fails() {
		let who = Account {
			balance: U256::from(21_000_000_000_000u128),
			nonce: U256::zero(),
		};
		// Transactional
		let test = default_transaction(true);
		let res = test.with_balance_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::BalanceTooLow);
		// Non-transactional
		let test = default_transaction(false);
		let res = test.with_balance_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::BalanceTooLow);
	}

	// Fee not set on transactional fails.
	#[test]
	fn validate_non_fee_transactional_fails() {
		let who = Account {
			balance: U256::from(21_000_000_000_001u128),
			nonce: U256::zero(),
		};
		let test = transaction_none_fee(true);
		let res = test.with_balance_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::InvalidFeeInput);
	}

	// Fee not set on non-transactional succeeds.
	#[test]
	fn validate_non_fee_non_transactional_succeeds() {
		let who = Account {
			balance: U256::from(0u8),
			nonce: U256::zero(),
		};
		let test = transaction_none_fee(false);
		let res = test.with_balance_for(&who);
		assert!(res.is_ok());
	}

	// Account balance is matched against max_fee_per_gas (without txn tip)
	#[test]
	fn validate_balance_regardless_of_base_fee() {
		let who = Account {
			// sufficient for base_fee, but not for max_fee_per_gas
			balance: U256::from(21_000_000_000_001u128),
			nonce: U256::zero(),
		};
		let with_tip = false;
		let test = transaction_max_fee_high(with_tip);
		let res = test.with_balance_for(&who);
		assert!(res.is_err());
	}

	// Account balance is matched against max_fee_per_gas (with txn tip)
	#[test]
	fn validate_balance_regardless_of_effective_gas_price() {
		let who = Account {
			// sufficient for (base_fee + tip), but not for max_fee_per_gas
			balance: U256::from(42_000_000_000_001u128),
			nonce: U256::zero(),
		};
		let with_tip = true;
		let test = transaction_max_fee_high(with_tip);
		let res = test.with_balance_for(&who);
		assert!(res.is_err());
	}

	// Account balance is matched against the provided gas_price for Legacy transactions.
	#[test]
	fn validate_balance_for_legacy_transaction_succeeds() {
		let who = Account {
			balance: U256::from(21_000_000_000_001u128),
			nonce: U256::zero(),
		};
		let test = legacy_transaction();
		let res = test.with_balance_for(&who);
		assert!(res.is_ok());
	}

	// Account balance is matched against the provided gas_price for Legacy transactions.
	#[test]
	fn validate_balance_for_legacy_transaction_fails() {
		let who = Account {
			balance: U256::from(21_000_000_000_000u128),
			nonce: U256::zero(),
		};
		let test = legacy_transaction();
		let res = test.with_balance_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::BalanceTooLow);
	}

	// Transaction with invalid fee input - mixing gas_price and max_fee_per_gas.
	#[test]
	fn validate_balance_with_invalid_fee_input() {
		let who = Account {
			balance: U256::from(21_000_000_000_001u128),
			nonce: U256::zero(),
		};
		// Fails for transactional.
		let is_transactional = true;
		let test = invalid_transaction_mixed_fees(is_transactional);
		let res = test.with_balance_for(&who);
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::InvalidFeeInput);
		// Succeeds for non-transactional.
		let is_transactional = false;
		let test = invalid_transaction_mixed_fees(is_transactional);
		let res = test.with_balance_for(&who);
		assert!(res.is_ok());
	}

	// Transaction with invalid fee input - mixing gas_price and max_fee_per_gas.
	#[test]
	fn validate_base_fee_with_invalid_fee_input() {
		// Fails for transactional.
		let is_transactional = true;
		let test = invalid_transaction_mixed_fees(is_transactional);
		let res = test.with_base_fee();
		assert!(res.is_err());
		assert_eq!(res.unwrap_err(), TestError::InvalidFeeInput);
		// Succeeds for non-transactional.
		let is_transactional = false;
		let test = invalid_transaction_mixed_fees(is_transactional);
		let res = test.with_base_fee();
		assert!(res.is_ok());
	}
}
