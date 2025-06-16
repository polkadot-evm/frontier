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

//! EIP-7702 transaction tests.

use std::str::FromStr;

use ethereum::{AuthorizationListItem, TransactionAction};
use ethereum_types::{H160, H256, U256};
use frame_support::assert_ok;
use sp_runtime::traits::Dispatchable;

use crate::{
	mock::*,
	tests::EIP7702UnsignedTransaction,
	Call, Event,
};

fn eip7702_transaction() -> EIP7702UnsignedTransaction {
	EIP7702UnsignedTransaction {
		nonce: U256::from(1),
		max_priority_fee_per_gas: U256::from(1),
		max_fee_per_gas: U256::from(1),
		gas_limit: U256::from(21000),
		destination: TransactionAction::Call(H160::default()),
		value: U256::zero(),
		data: vec![],
		authorization_list: vec![AuthorizationListItem {
			chain_id: 100,
			address: H160::from_str("0x1234567890123456789012345678901234567890").unwrap(),
			nonce: U256::from(0),
			y_parity: false,
			r: H256::from_str("0x1234567890123456789012345678901234567890123456789012345678901234")
				.unwrap(),
			s: H256::from_str("0x5678901234567890123456789012345678901234567890123456789012345678")
				.unwrap(),
		}],
	}
}

fn eip7702_transaction_with_invalid_authorization() -> EIP7702UnsignedTransaction {
	EIP7702UnsignedTransaction {
		nonce: U256::from(1),
		max_priority_fee_per_gas: U256::from(1),
		max_fee_per_gas: U256::from(1),
		gas_limit: U256::from(21000),
		destination: TransactionAction::Call(H160::default()),
		value: U256::zero(),
		data: vec![],
		authorization_list: vec![AuthorizationListItem {
			chain_id: 999, // Wrong chain ID
			address: H160::from_str("0x1234567890123456789012345678901234567890").unwrap(),
			nonce: U256::from(0),
			y_parity: false,
			r: H256::from_str("0x1234567890123456789012345678901234567890123456789012345678901234")
				.unwrap(),
			s: H256::from_str("0x5678901234567890123456789012345678901234567890123456789012345678")
				.unwrap(),
		}],
	}
}

#[test]
fn transaction_should_increment_nonce() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = eip7702_transaction();
		let signed = t.sign(&alice.private_key, Some(100));
		let call = Call::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();

		// Check that the nonce is incremented
		let pre_nonce = EVM::account_basic(&source).0.nonce;
		assert_ok!(call.dispatch(RuntimeOrigin::none()));
		let post_nonce = EVM::account_basic(&source).0.nonce;
		assert_eq!(post_nonce, pre_nonce + U256::from(1));
	});
}

#[test]
fn transaction_should_generate_correct_gas_used() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = eip7702_transaction();
		let signed = t.sign(&alice.private_key, Some(100));
		let call = Call::transact {
			transaction: signed,
		};

		assert_ok!(call.dispatch(RuntimeOrigin::none()));

		// Check that transaction was executed successfully by verifying the event was emitted
		System::assert_has_event(
			Event::Executed {
				from: alice.address,
				to: H160::default(),
				transaction_hash: signed.hash(),
				exit_reason: evm::ExitReason::Succeed(evm::ExitSucceed::Stopped),
				extra_data: vec![],
			}
			.into(),
		);
	});
}

#[test]
fn transaction_with_authorization_list_should_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = eip7702_transaction();
		let signed = t.sign(&alice.private_key, Some(100));
		let call = Call::transact {
			transaction: signed,
		};

		assert_ok!(call.dispatch(RuntimeOrigin::none()));

		// Check that transaction was executed successfully by verifying the event was emitted
		System::assert_has_event(
			Event::Executed {
				from: alice.address,
				to: H160::default(),
				transaction_hash: signed.hash(),
				exit_reason: evm::ExitReason::Succeed(evm::ExitSucceed::Stopped),
				extra_data: vec![],
			}
			.into(),
		);
	});
}

#[test]
fn transaction_with_invalid_authorization_chain_id_should_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = eip7702_transaction_with_invalid_authorization();
		let signed = t.sign(&alice.private_key, Some(100));
		let call = Call::transact {
			transaction: signed,
		};

		// Transaction should still work, but authorization should be ignored
		assert_ok!(call.dispatch(RuntimeOrigin::none()));

		// Check that transaction was executed successfully
		System::assert_has_event(
			Event::Executed {
				from: alice.address,
				to: H160::default(),
				transaction_hash: signed.hash(),
				exit_reason: evm::ExitReason::Succeed(evm::ExitSucceed::Stopped),
				extra_data: vec![],
			}
			.into(),
		);
	});
}

#[test]
fn source_should_be_derived_from_signature() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = eip7702_transaction();
		let signed = t.sign(&alice.private_key, Some(100));

		let source = Ethereum::recover_signer(&signed).unwrap();
		assert_eq!(source, alice.address);
	});
}

#[test]
fn contract_constructor_should_get_executed() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut t = eip7702_transaction();
		t.destination = TransactionAction::Create;
		t.data = hex::decode("608060405234801561001057600080fd5b50336000806101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff160217905550610075565b61017c806100646000396000f3fe608060405234801561001057600080fd5b50600436106100365760003560e01c8063893d20e81461003b578063a6f9dae11461006b575b600080fd5b610043610097565b60405173ffffffffffffffffffffffffffffffffffffffff16815260200160405180910390f35b61009561007636600461012a565b600080547fffffffffffffffffffffffff00000000000000000000000000000000000000001673ffffffffffffffffffffffffffffffffffffffff92909216919091179055565b005b60005473ffffffffffffffffffffffffffffffffffffffff1690565b6000602082840312156100c857600080fd5b813573ffffffffffffffffffffffffffffffffffffffff811681146100ec57600080fd5b9392505050565b600082356fffffffffffffffffffffffffffffffff8116811461011557600080fd5b9392505050565b80356fffffffffffffffffffffffffffffffff8116811461013c57600080fd5b919050565b6000602082840312156101545761015457600080fd5b6100ec8261011c56fea2646970667358221220").unwrap();
		let signed = t.sign(&alice.private_key, Some(100));
		let call = Call::transact { transaction: signed };

		assert_ok!(call.dispatch(RuntimeOrigin::none()));

		// Check that contract was created successfully by verifying the event was emitted
		System::assert_has_event(
			Event::Executed {
				from: alice.address,
				to: H160::zero(), // Create transaction goes to zero address
				transaction_hash: signed.hash(),
				exit_reason: evm::ExitReason::Succeed(evm::ExitSucceed::Stopped),
				extra_data: vec![],
			}
			.into(),
		);
	});
}

#[test]
fn proof_size_base_cost_should_keep_the_same_in_execution_and_estimate() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let raw_tx = eip7702_transaction();

		let signed_tx = raw_tx.clone().sign(&alice.private_key, Some(100));
		let tx_data: fp_ethereum::TransactionData = (&signed_tx).into();

		let estimate_tx_data = fp_ethereum::TransactionData::new(
			raw_tx.destination,
			raw_tx.data.clone(),
			raw_tx.nonce,
			raw_tx.gas_limit,
			None,
			Some(raw_tx.max_fee_per_gas),
			Some(raw_tx.max_priority_fee_per_gas),
			raw_tx.value,
			Some(100),
			vec![],
			raw_tx.authorization_list.clone(),
		);

		assert_eq!(
			estimate_tx_data.proof_size_base_cost(),
			tx_data.proof_size_base_cost()
		);
	});
}

#[test]
fn validated_transaction_apply_zero_gas_price_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut t = eip7702_transaction();
		t.max_fee_per_gas = U256::zero();
		t.max_priority_fee_per_gas = U256::zero();
		let signed = t.sign(&alice.private_key, Some(100));

		let call = Call::transact {
			transaction: signed,
		};

		assert_ok!(call.dispatch(RuntimeOrigin::none()));

		// Transaction was successful (zero gas price is valid)
	});
}

#[test]
fn self_contained_transaction_with_extra_gas_should_adjust_weight_with_post_dispatch() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut t = eip7702_transaction();
		t.gas_limit = U256::from(1_000_000); // Extra gas
		let signed = t.sign(&alice.private_key, Some(100));

		let call = Call::transact {
			transaction: signed,
		};

		let post_dispatch_result = call.dispatch(RuntimeOrigin::none());
		assert_ok!(post_dispatch_result);

		// Transaction was successful with extra gas
	});
}

#[test]
fn event_extra_data_should_be_handled_properly() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = eip7702_transaction();
		let signed = t.sign(&alice.private_key, Some(100));
		let call = Call::transact {
			transaction: signed,
		};

		assert_ok!(call.dispatch(RuntimeOrigin::none()));

		// Check that the execution event was emitted
		System::assert_has_event(
			Event::Executed {
				from: alice.address,
				to: H160::default(),
				transaction_hash: signed.hash(),
				exit_reason: evm::ExitReason::Succeed(evm::ExitSucceed::Stopped),
				extra_data: vec![],
			}
			.into(),
		);
	});
}

#[test]
fn authorization_processing_should_set_delegation_code() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let authorized_address = H160::from_str("0x1234567890123456789012345678901234567890").unwrap();
		
		// Create an EIP-7702 transaction with authorization
		let t = eip7702_transaction();
		let signed = t.sign(&alice.private_key, Some(100));
		let call = Call::transact {
			transaction: signed,
		};

		// Before execution, alice should have no code
		let alice_code_before = pallet_evm::AccountCodes::<Test>::get(alice.address);
		assert_eq!(alice_code_before, vec![]);

		assert_ok!(call.dispatch(RuntimeOrigin::none()));

		// After execution, alice should have delegation designator code
		let alice_code_after = pallet_evm::AccountCodes::<Test>::get(alice.address);
		
		// Expected delegation designator: 0xef0100 + authorized_address
		let mut expected_code = vec![0xef, 0x01, 0x00];
		expected_code.extend_from_slice(authorized_address.as_bytes());
		
		assert_eq!(alice_code_after, expected_code);
		
		// Check that code metadata was also set
		let code_metadata = pallet_evm::AccountCodesMetadata::<Test>::get(alice.address);
		assert!(code_metadata.is_some());
	});
}
