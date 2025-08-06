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

//! Shielding integration tests

use super::*;
use fp_ethereum::ValidatedTransaction;
use pallet_evm::AddressMapping;
use crate::mock;
use evm::{ExitReason, ExitError};

#[test]
fn shielding_with_designated_address_works() {
	let initial_balance = 20_000_000;
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, initial_balance);
	let alice = &pairs[0];
	let _bob = &pairs[1];
	let substrate_alice =
		<Test as pallet_evm::Config>::AddressMapping::into_account_id(alice.address);

	ext.execute_with(|| {
		// Use the same config as the EVM pallet (CANCUN_CONFIG)
		let config = evm::Config::cancun();
		let note = H256::from_slice(&[1u8; 32]);
		
		// Then simulate the EVM transaction that would transfer funds
		let transaction = mock::LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::zero(),
			gas_limit: U256::from(900_000),
			action: ethereum::TransactionAction::Call(config.shielding_pool_address),
			value: config.shielding_unit_amount,
			input: note.as_bytes().to_vec(),
		}
		.sign(&alice.private_key);

		assert_ok!(crate::ValidatedTransaction::<Test>::apply(
			alice.address,
			transaction
		));

		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(&substrate_alice), initial_balance - config.shielding_unit_amount.as_u64());
		
		assert_eq!(::shielding::Pallet::<Test>::notes(0), Some(note));
	});
}

#[test]
fn shielding_with_multiple_accounts_works() {
	let initial_balance = 20_000_000;
	let (pairs, mut ext) = new_test_ext_with_initial_balance(3, initial_balance);
	let alice = &pairs[0];
	let bob = &pairs[1];
	let charlie = &pairs[2];

	let substrate_alice = <Test as pallet_evm::Config>::AddressMapping::into_account_id(alice.address);
	let substrate_bob = <Test as pallet_evm::Config>::AddressMapping::into_account_id(bob.address);
	let substrate_charlie = <Test as pallet_evm::Config>::AddressMapping::into_account_id(charlie.address);

	ext.execute_with(|| {
		let config = evm::Config::cancun();
		let note1 = H256::from_slice(&[1u8; 32]);
		let note2 = H256::from_slice(&[2u8; 32]);
		let note3 = H256::from_slice(&[3u8; 32]);

		// Shield from Alice
		let transaction1 = mock::LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::zero(),
			gas_limit: U256::from(900_000),
			action: ethereum::TransactionAction::Call(config.shielding_pool_address),
			value: config.shielding_unit_amount,
			input: note1.as_bytes().to_vec(),
		}
		.sign(&alice.private_key);

		// Shield from Bob
		let transaction2 = mock::LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::zero(),
			gas_limit: U256::from(900_000),
			action: ethereum::TransactionAction::Call(config.shielding_pool_address),
			value: config.shielding_unit_amount,
			input: note2.as_bytes().to_vec(),
		}
		.sign(&bob.private_key);

		// Shield from Charlie
		let transaction3 = mock::LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::zero(),
			gas_limit: U256::from(900_000),
			action: ethereum::TransactionAction::Call(config.shielding_pool_address),
			value: config.shielding_unit_amount,
			input: note3.as_bytes().to_vec(),
		}
		.sign(&charlie.private_key);

		// Apply all transactions
		assert_ok!(crate::ValidatedTransaction::<Test>::apply(
			alice.address,
			transaction1
		));
		assert_ok!(crate::ValidatedTransaction::<Test>::apply(
			bob.address,
			transaction2
		));
		assert_ok!(crate::ValidatedTransaction::<Test>::apply(
			charlie.address,
			transaction3
		));

		// Verify balances were deducted
		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(&substrate_alice), initial_balance - config.shielding_unit_amount.as_u64());
		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(&substrate_bob), initial_balance - config.shielding_unit_amount.as_u64());
		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(&substrate_charlie), initial_balance - config.shielding_unit_amount.as_u64());

		// Verify notes were stored
		assert_eq!(::shielding::Pallet::<Test>::notes(0), Some(note1));
		assert_eq!(::shielding::Pallet::<Test>::notes(1), Some(note2));
		assert_eq!(::shielding::Pallet::<Test>::notes(2), Some(note3));
	});
}

#[test]
fn shielding_fails_when_pool_is_full() {
	let initial_balance = 100_000_000; // Large balance to fill the pool
	let (pairs, mut ext) = new_test_ext_with_initial_balance(1, initial_balance);
	let alice = &pairs[0];
	let substrate_alice =
		<Test as pallet_evm::Config>::AddressMapping::into_account_id(alice.address);

	ext.execute_with(|| {
		let config = evm::Config::cancun();
		
		// Calculate how many notes we can add (2^4 = 16)
		let max_notes = 1 << 4; // MaxTreeDepth = 4
		
		// Fill the shielding pool to capacity
		for i in 0..max_notes+1 {
			let note = H256::from_slice(&[(i % 256) as u8; 32]);
			
			let transaction = mock::LegacyUnsignedTransaction {
				nonce: U256::from(i),
				gas_price: U256::zero(),
				gas_limit: U256::from(900_000),
				action: ethereum::TransactionAction::Call(config.shielding_pool_address),
				value: config.shielding_unit_amount,
				input: note.as_bytes().to_vec(),
			}
			.sign(&alice.private_key);

			// All transactions should succeed until the pool is full
			assert_ok!(crate::ValidatedTransaction::<Test>::apply(
				alice.address,
				transaction
			));
		}
		
		// Verify the pool is full
		assert_eq!(::shielding::Pallet::<Test>::note_count(), max_notes as u64);
		
		// Check balance before the failing transaction
		let balance_before_fail = pallet_balances::Pallet::<Test>::free_balance(&substrate_alice);
		
		// Try to add one more note - this should fail
		let overflow_note = H256::from_slice(&[255u8; 32]);
		let overflow_transaction = mock::LegacyUnsignedTransaction {
			nonce: U256::from(max_notes),
			gas_price: U256::zero(),
			gas_limit: U256::from(900_000),
			action: ethereum::TransactionAction::Call(config.shielding_pool_address),
			value: config.shielding_unit_amount,
			input: overflow_note.as_bytes().to_vec(),
		}
		.sign(&alice.private_key);

		// This transaction should fail because the pool is full
		let result = crate::ValidatedTransaction::<Test>::apply(
			alice.address,
			overflow_transaction
		);
		
		// The transaction should succeed at the pallet level but fail at the EVM level
		assert!(result.is_ok());
		
		// Extract the exit reason from the result
		let (_, call_info) = result.unwrap();
		let exit_reason = match call_info {
			CallOrCreateInfo::Call(info) => info.exit_reason,
			CallOrCreateInfo::Create(info) => info.exit_reason,
		};
		
		// The EVM execution should fail with the shielding error
		assert!(matches!(exit_reason, ExitReason::Error(ExitError::Other(_))));
		
		// Verify the note count didn't increase
		assert_eq!(::shielding::Pallet::<Test>::note_count(), max_notes as u64);
		
		// Verify the overflow note was not added
		assert_eq!(::shielding::Pallet::<Test>::notes(max_notes as u64), None);
		
		// Verify the balance was NOT transferred (should remain the same)
		let balance_after_fail = pallet_balances::Pallet::<Test>::free_balance(&substrate_alice);
		assert_eq!(balance_after_fail, balance_before_fail, "Balance should not be deducted when shielding fails");
		
		// Verify the shielding pool balance didn't increase
		let shielding_pool_account_id = <Test as pallet_evm::Config>::AddressMapping::into_account_id(config.shielding_pool_address);
		let shielding_pool_balance = pallet_balances::Pallet::<Test>::free_balance(&shielding_pool_account_id);
		let expected_shielding_pool_balance = max_notes as u64 * config.shielding_unit_amount.as_u64();
		assert_eq!(shielding_pool_balance, expected_shielding_pool_balance, "Shielding pool balance should not increase when transaction fails");
	});
}

