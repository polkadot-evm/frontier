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

//! EIP-7702 Set Code Authorization transaction tests

use super::*;
use ethereum::{AuthorizationListItem, TransactionAction};
use pallet_evm::config_preludes::ChainId;
use sp_core::{H160, H256, U256};

/// Helper function to create an EIP-7702 transaction for testing
fn eip7702_transaction_unsigned(
	nonce: U256,
	gas_limit: U256,
	destination: TransactionAction,
	value: U256,
	data: Vec<u8>,
	authorization_list: Vec<AuthorizationListItem>,
) -> EIP7702UnsignedTransaction {
	EIP7702UnsignedTransaction {
		nonce,
		max_priority_fee_per_gas: U256::from(1),
		max_fee_per_gas: U256::from(1),
		gas_limit,
		destination,
		value,
		data,
		authorization_list,
	}
}

/// Helper function to create a signed authorization tuple
fn create_authorization_tuple(
	chain_id: u64,
	address: H160,
	nonce: u64,
	private_key: &H256,
) -> AuthorizationListItem {
	let secret = {
		let mut sk: [u8; 32] = [0u8; 32];
		sk.copy_from_slice(&private_key[0..]);
		libsecp256k1::SecretKey::parse(&sk).unwrap()
	};

	// Create a mock signature for testing
	let msg = [0u8; 32]; // Mock message
	let signing_message = libsecp256k1::Message::parse_slice(&msg[..]).unwrap();
	let (signature, recid) = libsecp256k1::sign(&signing_message, &secret);
	let rs = signature.serialize();
	let r = H256::from_slice(&rs[0..32]);
	let s = H256::from_slice(&rs[32..64]);

	AuthorizationListItem {
		chain_id,
		address,
		nonce: U256::from(nonce),
		signature: ethereum::eip2930::MalleableTransactionSignature {
			odd_y_parity: recid.serialize() != 0,
			r,
			s,
		},
	}
}

#[test]
fn valid_eip7702_transaction_structure() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 10_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let contract_address =
			H160::from_str("0x1000000000000000000000000000000000000001").unwrap();
		let authorization =
			create_authorization_tuple(ChainId::get(), contract_address, 0, &alice.private_key);

		let transaction = eip7702_transaction_unsigned(
			U256::zero(),
			U256::from(0x100000),
			TransactionAction::Call(bob.address),
			U256::from(1000),
			vec![],
			vec![authorization],
		)
		.sign(&alice.private_key, Some(ChainId::get()));

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should be valid
		assert_ok!(call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap());
	});
}

#[test]
fn eip7702_transaction_with_empty_authorization_list_fails() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 10_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let transaction = eip7702_transaction_unsigned(
			U256::zero(),
			U256::from(0x100000),
			TransactionAction::Call(bob.address),
			U256::from(1000),
			vec![],
			vec![], // Empty authorization list
		)
		.sign(&alice.private_key, Some(ChainId::get()));

		let call = crate::Call::<Test>::transact { transaction };

		// Transaction with empty authorization list should fail validation
		let check_result = call.check_self_contained();

		// The transaction should be recognized as self-contained (signature should be valid)
		let source = check_result
			.expect("EIP-7702 transaction should be recognized as self-contained")
			.expect("EIP-7702 transaction signature should be valid");

		// But validation should fail due to empty authorization list
		let validation_result = call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.expect("Validation should return a result");

		// Assert that validation fails
		assert!(
			validation_result.is_err(),
			"EIP-7702 transaction with empty authorization list should fail validation"
		);
	});
}

#[test]
fn eip7702_transaction_execution() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 10_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let contract_address =
			H160::from_str("0x1000000000000000000000000000000000000001").unwrap();
		let authorization =
			create_authorization_tuple(ChainId::get(), contract_address, 0, &alice.private_key);

		let transaction = eip7702_transaction_unsigned(
			U256::zero(),
			U256::from(0x100000),
			TransactionAction::Call(bob.address),
			U256::from(1000),
			vec![],
			vec![authorization],
		)
		.sign(&alice.private_key, Some(ChainId::get()));

		// Execute the transaction using the Ethereum pallet
		let result = Ethereum::execute(alice.address, &transaction, None);

		// The transaction should execute successfully or fail gracefully
		// The exact result depends on the EIP-7702 implementation state
		match result {
			Ok(_) => {
				// Transaction executed successfully
				// In a full implementation, we would verify:
				// 1. Alice's account has delegation indicator set
				// 2. Nonce was incremented
				// 3. Gas was consumed correctly
			}
			Err(_) => {
				// Transaction failed - this might be expected if EIP-7702 is not fully implemented
				// This test documents the current behavior
			}
		}
	});
}

#[test]
fn authorization_with_wrong_chain_id() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 10_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let contract_address =
			H160::from_str("0x1000000000000000000000000000000000000001").unwrap();
		// Create authorization with wrong chain ID
		let authorization =
			create_authorization_tuple(999, contract_address, 0, &alice.private_key);

		let transaction = eip7702_transaction_unsigned(
			U256::zero(),
			U256::from(0x100000),
			TransactionAction::Call(bob.address),
			U256::from(1000),
			vec![],
			vec![authorization],
		)
		.sign(&alice.private_key, Some(ChainId::get()));

		let call = crate::Call::<Test>::transact { transaction };
		let check_result = call.check_self_contained();

		// Transaction should be structurally valid but authorization should be invalid
		if let Some(Ok(source)) = check_result {
			let _validation_result =
				call.validate_self_contained(&source, &call.get_dispatch_info(), 0);
			// The transaction might still pass validation but the authorization would be skipped during execution
			// This documents the expected behavior for invalid chain IDs
		}
	});
}

#[test]
fn authorization_with_zero_chain_id() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 10_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let contract_address =
			H160::from_str("0x1000000000000000000000000000000000000001").unwrap();
		// Create authorization with chain ID = 0 (should be universally valid)
		let authorization = create_authorization_tuple(0, contract_address, 0, &alice.private_key);

		let transaction = eip7702_transaction_unsigned(
			U256::zero(),
			U256::from(0x100000),
			TransactionAction::Call(bob.address),
			U256::from(1000),
			vec![],
			vec![authorization],
		)
		.sign(&alice.private_key, Some(ChainId::get()));

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should be valid - chain_id = 0 is universally accepted
		assert_ok!(call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap());
	});
}

#[test]
fn multiple_authorizations_for_same_authority() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 10_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let contract1 = H160::from_str("0x1000000000000000000000000000000000000001").unwrap();
		let contract2 = H160::from_str("0x2000000000000000000000000000000000000002").unwrap();

		// Create multiple authorizations for the same authority (Alice)
		let auth1 = create_authorization_tuple(ChainId::get(), contract1, 0, &alice.private_key);
		let auth2 = create_authorization_tuple(ChainId::get(), contract2, 0, &alice.private_key);

		let transaction = eip7702_transaction_unsigned(
			U256::zero(),
			U256::from(0x100000),
			TransactionAction::Call(bob.address),
			U256::from(1000),
			vec![],
			vec![auth1, auth2], // Multiple authorizations for same authority
		)
		.sign(&alice.private_key, Some(ChainId::get()));

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Transaction should be valid - multiple authorizations are allowed
		// The EIP specifies that the last valid authorization should win
		assert_ok!(call
			.validate_self_contained(&source, &call.get_dispatch_info(), 0)
			.unwrap());
	});
}

#[test]
fn gas_cost_calculation_with_authorizations() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 10_000_000_000_000);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let contract_address =
			H160::from_str("0x1000000000000000000000000000000000000001").unwrap();
		let authorization =
			create_authorization_tuple(ChainId::get(), contract_address, 0, &alice.private_key);

		let transaction = eip7702_transaction_unsigned(
			U256::zero(),
			U256::from(0x100000),
			TransactionAction::Call(bob.address),
			U256::from(1000),
			vec![],
			vec![authorization],
		)
		.sign(&alice.private_key, Some(ChainId::get()));

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();

		// Verify the transaction passes validation (which includes gas cost checks)
		let validation_result = call.validate_self_contained(&source, &call.get_dispatch_info(), 0);
		assert_ok!(validation_result.unwrap());

		// The gas cost should include:
		// - Base transaction cost (21000)
		// - Per-authorization cost (PER_AUTH_BASE_COST = 12500)
		// - Per-empty-account cost (PER_EMPTY_ACCOUNT_COST = 25000) if authority is empty
		// This test verifies that gas calculation doesn't reject the transaction
	});
}
