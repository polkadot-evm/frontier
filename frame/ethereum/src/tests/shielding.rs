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

//! Consensus extension module tests for BABE consensus.

use super::*;
use fp_ethereum::{ValidatedTransaction};
use pallet_evm::AddressMapping;




#[test]
fn shielding_with_designated_address_works() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 2_000_000);
	let alice = &pairs[0];
	let _bob = &pairs[1];
	let substrate_alice =
		<Test as pallet_evm::Config>::AddressMapping::into_account_id(alice.address);

	println!("alice: {:?}", alice.address);

	ext.execute_with(|| {
		let config = evm::Config::frontier();
		let transaction = LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::zero(),
			gas_limit: U256::from(210_000),
			action: ethereum::TransactionAction::Call(config.shielding_pool_address),
			value: config.shielding_unit_amount,
			input: vec![1u8; 32],
		}
		.sign(&alice.private_key);

		assert_ok!(crate::ValidatedTransaction::<Test>::apply(
			alice.address,
			transaction
		));
		// Alice didn't pay fees, transfer 100 to Bob.
		assert_eq!(Balances::free_balance(&substrate_alice), 2_000_000 - config.shielding_unit_amount.as_u64());



	});
}
