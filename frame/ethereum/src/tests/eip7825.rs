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

//! EIP-7825 Transaction Gas Limit Cap tests

use super::*;
use fp_evm::{TransactionValidationError, MAX_TRANSACTION_GAS_LIMIT};

/// Helper function to create a legacy transaction with a specific gas limit
fn legacy_transaction_with_gas_limit(
	nonce: U256,
	gas_limit: U256,
	to: H160,
	value: U256,
) -> LegacyUnsignedTransaction {
	LegacyUnsignedTransaction {
		nonce,
		gas_price: U256::from(1),
		gas_limit,
		action: TransactionAction::Call(to),
		value,
		input: vec![],
	}
}

/// Helper function to create an EIP-1559 transaction with a specific gas limit
fn eip1559_transaction_with_gas_limit(
	nonce: U256,
	gas_limit: U256,
	to: H160,
	value: U256,
) -> EIP1559UnsignedTransaction {
	EIP1559UnsignedTransaction {
		nonce,
		max_priority_fee_per_gas: U256::from(1),
		max_fee_per_gas: U256::from(1),
		gas_limit,
		action: TransactionAction::Call(to),
		value,
		input: vec![],
	}
}

#[test]
fn eip7825_transaction_at_exactly_cap_succeeds() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 100_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		// Transaction at exactly the EIP-7825 cap should pass validation
		let transaction = legacy_transaction_with_gas_limit(
			U256::zero(),
			U256::from(MAX_TRANSACTION_GAS_LIMIT), // Exactly at cap (16,777,216)
			bob.address,
			U256::from(1000),
		)
		.sign(&alice.private_key);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should be valid
		assert_ok!(call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap());
	});
}

#[test]
fn eip7825_transaction_exceeds_cap_by_one_fails() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 100_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		// Transaction exceeding cap by 1 gas should fail
		let transaction = legacy_transaction_with_gas_limit(
			U256::zero(),
			U256::from(MAX_TRANSACTION_GAS_LIMIT + 1), // 1 over cap
			bob.address,
			U256::from(1000),
		)
		.sign(&alice.private_key);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should fail validation
		let validation_result = call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap();

		assert!(validation_result.is_err());
		assert_eq!(
			validation_result.unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				TransactionValidationError::TransactionGasLimitExceedsCap as u8
			))
		);
	});
}

#[test]
fn eip7825_transaction_well_under_cap_succeeds() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 100_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		// Transaction with standard transfer gas (21,000) should pass
		let transaction = legacy_transaction_with_gas_limit(
			U256::zero(),
			U256::from(21_000u64),
			bob.address,
			U256::from(1000),
		)
		.sign(&alice.private_key);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should be valid
		assert_ok!(call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap());
	});
}

#[test]
fn eip7825_transaction_well_over_cap_fails() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 100_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		// Transaction with gas limit nearly double the cap should fail
		let transaction = legacy_transaction_with_gas_limit(
			U256::zero(),
			U256::from(30_000_000u64), // Nearly double the 16,777,216 cap
			bob.address,
			U256::from(1000),
		)
		.sign(&alice.private_key);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should fail validation
		let validation_result = call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap();

		assert!(validation_result.is_err());
		assert_eq!(
			validation_result.unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				TransactionValidationError::TransactionGasLimitExceedsCap as u8
			))
		);
	});
}

#[test]
fn eip7825_eip1559_transaction_exceeds_cap_fails() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 100_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		// EIP-1559 transaction exceeding cap should also fail
		let transaction = eip1559_transaction_with_gas_limit(
			U256::zero(),
			U256::from(MAX_TRANSACTION_GAS_LIMIT + 1),
			bob.address,
			U256::from(1000),
		)
		.sign(&alice.private_key, None);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should fail validation
		let validation_result = call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap();

		assert!(validation_result.is_err());
		assert_eq!(
			validation_result.unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				TransactionValidationError::TransactionGasLimitExceedsCap as u8
			))
		);
	});
}

#[test]
fn eip7825_block_validation_rejects_exceeding_cap() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 100_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		// Transaction exceeding cap should fail block validation
		let transaction = legacy_transaction_with_gas_limit(
			U256::zero(),
			U256::from(MAX_TRANSACTION_GAS_LIMIT + 1),
			bob.address,
			U256::from(1000),
		)
		.sign(&alice.private_key);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Pre-dispatch (block validation) should fail
		let pre_dispatch_result =
			call.pre_dispatch_self_contained(&source, &call.get_dispatch_info(), 0);

		assert!(pre_dispatch_result.is_some());
		let result = pre_dispatch_result.unwrap();
		assert!(result.is_err());
		assert_eq!(
			result.unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				TransactionValidationError::TransactionGasLimitExceedsCap as u8
			))
		);
	});
}

#[test]
fn eip7825_constant_value_is_correct() {
	// Verify the constant is correctly set to 2^24
	assert_eq!(MAX_TRANSACTION_GAS_LIMIT, 16_777_216);
	assert_eq!(MAX_TRANSACTION_GAS_LIMIT, 1 << 24);
}
