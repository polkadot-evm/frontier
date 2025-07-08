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

use std::panic;

use super::*;
use ethereum::{AuthorizationListItem, TransactionAction};
use pallet_evm::{config_preludes::ChainId, AddressMapping};
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
	use rlp::RlpStream;

	let secret = {
		let mut sk: [u8; 32] = [0u8; 32];
		sk.copy_from_slice(&private_key[0..]);
		libsecp256k1::SecretKey::parse(&sk).unwrap()
	};

	// Create the proper EIP-7702 authorization message
	// msg = keccak(MAGIC || rlp([chain_id, address, nonce]))
	let magic: u8 = 0x05;
	let mut stream = RlpStream::new_list(3);
	stream.append(&chain_id);
	stream.append(&address);
	stream.append(&nonce);

	let mut msg_data = vec![magic];
	msg_data.extend_from_slice(&stream.out());

	let msg_hash = sp_io::hashing::keccak_256(&msg_data);
	let signing_message = libsecp256k1::Message::parse_slice(&msg_hash).unwrap();
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

		// Store initial account state for comparison
		let substrate_alice =
			<Test as pallet_evm::Config>::AddressMapping::into_account_id(alice.address);
		let substrate_bob =
			<Test as pallet_evm::Config>::AddressMapping::into_account_id(bob.address);
		let initial_alice_nonce = System::account_nonce(&substrate_alice);
		let initial_alice_balance = Balances::free_balance(&substrate_alice);
		let initial_bob_balance = Balances::free_balance(&substrate_bob);

		// Execute the transaction using the Ethereum pallet
		let result = Ethereum::execute(alice.address, &transaction, None);

		// Verify transaction execution and state changes
		let Ok(execution_info) = result else {
			panic!("Transaction execution failed")
		};

		// Transaction executed successfully - verify expected state changes

		// 1. Verify nonce was incremented (EIP-7702 authorization + transaction)
		let final_alice_nonce = System::account_nonce(&substrate_alice);
		assert_eq!(
			final_alice_nonce,
			initial_alice_nonce + 2,
			"Alice's nonce should be incremented by 2: +1 for EIP-7702 authorization, +1 for transaction"
		);

		// 2. Verify gas was consumed (execution_info contains gas usage)
		let (_, _, call_info) = execution_info;
		match call_info {
			CallOrCreateInfo::Call(call_info) => {
				assert!(
					call_info.used_gas.standard > U256::from(21000),
					"Gas usage should be at least the base transaction cost (21000)"
				);
			}
			CallOrCreateInfo::Create(create_info) => {
				assert!(
					create_info.used_gas.standard > U256::from(21000),
					"Gas usage should be at least the base transaction cost (21000)"
				);
			}
		}

		// 3. Verify value transfer occurred (1000 wei from Alice to Bob)
		let final_alice_balance = Balances::free_balance(&substrate_alice);
		let final_bob_balance = Balances::free_balance(&substrate_bob);

		// Alice should have paid the transaction value plus gas costs
		assert!(
			final_alice_balance < initial_alice_balance,
			"Alice's balance should decrease after paying for transaction"
		);

		// Bob should have received the transaction value
		assert_eq!(
			final_bob_balance,
			initial_bob_balance + 1000u64,
			"Bob should receive the transaction value (1000 wei)"
		);

		// 4. Verify authorization list was processed
		// Check if Alice's account now has the delegated code from the authorization
		let alice_code = pallet_evm::AccountCodes::<Test>::get(alice.address);
		let contract_code = pallet_evm::AccountCodes::<Test>::get(contract_address);

		// Debug information for understanding the current state
		println!("Alice's code length: {}", alice_code.len());
		println!("Contract address code length: {}", contract_code.len());
		println!("Alice's code: {:?}", alice_code);

		// According to EIP-7702, after processing an authorization, the authorizing account
		// should have code set to 0xef0100 || address (delegation designator)
		if !alice_code.is_empty() {
			// Check if this is a proper EIP-7702 delegation designator
			if alice_code.len() >= 23 && alice_code[0] == 0xef && alice_code[1] == 0x01 && alice_code[2] == 0x00 {
				// Extract the delegated address from the designation
				let delegated_address: H160 = H160::from_slice(&alice_code[3..23]);
				assert_eq!(
					delegated_address,
					contract_address,
					"Alice's account should delegate to the authorized contract address"
				);
				println!("✓ EIP-7702 delegation properly set up");
			} else {
				println!("Alice's code is not a proper EIP-7702 delegation designator");
				panic!("EIP-7702 authorization verification failed");
			}
		} else {
			// If no code is set, this might indicate the authorization wasn't processed
			// or the EIP-7702 implementation is not complete
			println!("⚠ Alice's account has no code after EIP-7702 authorization");
			println!("This may indicate the authorization wasn't processed or EIP-7702 is not fully implemented");
			panic!("EIP-7702 authorization verification failed");
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
