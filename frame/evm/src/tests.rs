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

#![cfg(test)]

use std::{collections::BTreeMap, str::FromStr};
// Frontier
use fp_evm::TransactionPov;
// Substrate
use frame_support::{
	assert_ok,
	traits::{LockIdentifier, LockableCurrency, WithdrawReasons},
};
use sp_core::Blake2Hasher;
use sp_io::TestExternalities;
use sp_runtime::BuildStorage;
use sp_state_machine::TrieBackendBuilder;
use sp_trie::{proof_size_extension::ProofSizeExt, recorder::Recorder};

use super::*;
use crate::mock::*;

type Balances = pallet_balances::Pallet<Test>;
#[allow(clippy::upper_case_acronyms)]
type EVM = Pallet<Test>;

pub fn new_test_ext() -> TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();

	let mut accounts = BTreeMap::new();
	accounts.insert(
		H160::from_str("1000000000000000000000000000000000000001").unwrap(),
		GenesisAccount {
			nonce: U256::from(1),
			balance: U256::from(1000000),
			storage: Default::default(),
			code: vec![
				0x00, // STOP
			],
		},
	);
	accounts.insert(
		H160::from_str("1000000000000000000000000000000000000002").unwrap(),
		GenesisAccount {
			nonce: U256::from(1),
			balance: U256::from(1000000),
			storage: Default::default(),
			code: vec![
				0xff, // INVALID
			],
		},
	);
	accounts.insert(
		H160::default(), // root
		GenesisAccount {
			nonce: U256::from(1),
			balance: U256::max_value(),
			storage: Default::default(),
			code: vec![],
		},
	);

	pallet_balances::GenesisConfig::<Test> {
		// Create the block author account with some balance.
		balances: vec![(
			H160::from_str("0x1234500000000000000000000000000000000000").unwrap(),
			12345,
		)],
	}
	.assimilate_storage(&mut storage)
	.expect("Pallet balances storage can be assimilated");

	crate::GenesisConfig::<Test> {
		accounts,
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	storage.into()
}

pub fn new_text_ext_with_recorder() -> TestExternalities {
	let text_ext = new_test_ext();

	let root = text_ext.backend.root().clone();
	let db = text_ext.backend.into_storage();
	let recorder: Recorder<Blake2Hasher> = Default::default();
	let backend_with_reorder = TrieBackendBuilder::new(db, root)
		.with_recorder(recorder.clone())
		.build();

	let mut test_ext_with_recorder: TestExternalities = TestExternalities::default();
	test_ext_with_recorder.backend = backend_with_reorder;
	test_ext_with_recorder.register_extension(ProofSizeExt::new(recorder));

	test_ext_with_recorder
}

#[test]
fn fail_call_return_ok() {
	new_test_ext().execute_with(|| {
		assert_ok!(EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::default(),
			1000000,
			U256::from(1_000_000_000),
			None,
			None,
			Vec::new(),
		));

		assert_ok!(EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000002").unwrap(),
			Vec::new(),
			U256::default(),
			1000000,
			U256::from(1_000_000_000),
			None,
			None,
			Vec::new(),
		));
	});
}

#[test]
fn fee_deduction() {
	new_test_ext().execute_with(|| {
		// Create an EVM address and the corresponding Substrate address that will be charged fees and refunded
		let evm_addr = H160::from_str("1000000000000000000000000000000000000003").unwrap();
		let substrate_addr = <Test as Config>::AddressMapping::into_account_id(evm_addr);

		// Seed account
		let _ = <Test as Config>::Currency::deposit_creating(&substrate_addr, 100);
		assert_eq!(Balances::free_balance(substrate_addr), 100);

		// Deduct fees as 10 units
		let imbalance = <<Test as Config>::OnChargeTransaction as OnChargeEVMTransaction<Test>>::withdraw_fee(&evm_addr, U256::from(10)).unwrap();
		assert_eq!(Balances::free_balance(substrate_addr), 90);

		// Refund fees as 5 units
		<<Test as Config>::OnChargeTransaction as OnChargeEVMTransaction<Test>>::correct_and_deposit_fee(&evm_addr, U256::from(5), U256::from(5), imbalance);
		assert_eq!(Balances::free_balance(substrate_addr), 95);
	});
}

#[test]
fn ed_0_refund_patch_works() {
	new_test_ext().execute_with(|| {
		// Verifies that the OnChargeEVMTransaction patch is applied and fixes a known bug in Substrate for evm transactions.
		// https://github.com/paritytech/substrate/issues/10117
		let evm_addr = H160::from_str("1000000000000000000000000000000000000003").unwrap();
		let substrate_addr = <Test as Config>::AddressMapping::into_account_id(evm_addr);

		let _ = <Test as Config>::Currency::deposit_creating(&substrate_addr, 21_777_000_000_000);
		assert_eq!(Balances::free_balance(substrate_addr), 21_777_000_000_000);

		let _ = EVM::call(
			RuntimeOrigin::root(),
			evm_addr,
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1_000_000_000),
			21776,
			U256::from(1_000_000_000),
			None,
			Some(U256::from(0)),
			Vec::new(),
		);
		// All that was due, was refunded.
		assert_eq!(Balances::free_balance(substrate_addr), 776_000_000_000);
	});
}

#[test]
fn ed_0_refund_patch_is_required() {
	new_test_ext().execute_with(|| {
		// This test proves that the patch is required, verifying that the current Substrate behaviour is incorrect
		// for ED 0 configured chains.
		let evm_addr = H160::from_str("1000000000000000000000000000000000000003").unwrap();
		let substrate_addr = <Test as Config>::AddressMapping::into_account_id(evm_addr);

		let _ = <Test as Config>::Currency::deposit_creating(&substrate_addr, 100);
		assert_eq!(Balances::free_balance(substrate_addr), 100);

		// Drain funds
		let _ =
			<<Test as Config>::OnChargeTransaction as OnChargeEVMTransaction<Test>>::withdraw_fee(
				&evm_addr,
				U256::from(100),
			)
			.unwrap();
		assert_eq!(Balances::free_balance(substrate_addr), 0);

		// Try to refund. With ED 0, although the balance is now 0, the account still exists.
		// So its expected that calling `deposit_into_existing` results in the AccountData to increase the Balance.
		//
		// Is not the case, and this proves that the refund logic needs to be handled taking this into account.
		assert!(
			<Test as Config>::Currency::deposit_into_existing(&substrate_addr, 5u32.into())
				.is_err()
		);
		// Balance didn't change, and should be 5.
		assert_eq!(Balances::free_balance(substrate_addr), 0);
	});
}

#[test]
fn find_author() {
	new_test_ext().execute_with(|| {
		let author = EVM::find_author();
		assert_eq!(
			author,
			H160::from_str("1234500000000000000000000000000000000000").unwrap()
		);
	});
}

#[test]
fn reducible_balance() {
	new_test_ext().execute_with(|| {
		let evm_addr = H160::from_str("1000000000000000000000000000000000000001").unwrap();
		let account_id = <Test as Config>::AddressMapping::into_account_id(evm_addr);
		let existential = ExistentialDeposit::get();

		// Genesis Balance.
		let genesis_balance = EVM::account_basic(&evm_addr).0.balance;

		// Lock identifier.
		let lock_id: LockIdentifier = *b"te/stlok";
		// Reserve some funds.
		let to_lock = 1000;
		Balances::set_lock(lock_id, &account_id, to_lock, WithdrawReasons::RESERVE);
		// Reducible is, as currently configured in `account_basic`, (balance - lock - existential).
		let reducible_balance = EVM::account_basic(&evm_addr).0.balance;
		assert_eq!(reducible_balance, (genesis_balance - to_lock - existential));
	});
}

#[test]
fn author_should_get_tip() {
	new_test_ext().execute_with(|| {
		let author = EVM::find_author();
		let before_tip = EVM::account_basic(&author).0.balance;
		let result = EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1),
			1000000,
			U256::from(2_000_000_000),
			Some(U256::from(1)),
			None,
			Vec::new(),
		);
		result.expect("EVM can be called");
		let after_tip = EVM::account_basic(&author).0.balance;
		assert_eq!(after_tip, (before_tip + 21000));
	});
}

#[test]
fn issuance_after_tip() {
	new_test_ext().execute_with(|| {
		let before_tip = <Test as Config>::Currency::total_issuance();
		let result = EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1),
			1000000,
			U256::from(2_000_000_000),
			Some(U256::from(1)),
			None,
			Vec::new(),
		);
		result.expect("EVM can be called");
		let after_tip = <Test as Config>::Currency::total_issuance();
		// Only base fee is burned
		let base_fee: u64 = <Test as Config>::FeeCalculator::min_gas_price()
			.0
			.unique_saturated_into();
		assert_eq!(after_tip, (before_tip - (base_fee * 21_000)));
	});
}

#[test]
fn author_same_balance_without_tip() {
	new_test_ext().execute_with(|| {
		let author = EVM::find_author();
		let before_tip = EVM::account_basic(&author).0.balance;
		let _ = EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::default(),
			1000000,
			U256::default(),
			None,
			None,
			Vec::new(),
		);
		let after_tip = EVM::account_basic(&author).0.balance;
		assert_eq!(after_tip, before_tip);
	});
}

#[test]
fn refunds_should_work() {
	new_test_ext().execute_with(|| {
		let before_call = EVM::account_basic(&H160::default()).0.balance;
		// Gas price is not part of the actual fee calculations anymore, only the base fee.
		//
		// Because we first deduct max_fee_per_gas * gas_limit (2_000_000_000 * 1000000) we need
		// to ensure that the difference (max fee VS base fee) is refunded.
		let _ = EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1),
			1000000,
			U256::from(2_000_000_000),
			None,
			None,
			Vec::new(),
		);
		let (base_fee, _) = <Test as Config>::FeeCalculator::min_gas_price();
		let total_cost = (U256::from(21_000) * base_fee) + U256::from(1);
		let after_call = EVM::account_basic(&H160::default()).0.balance;
		assert_eq!(after_call, before_call - total_cost);
	});
}

#[test]
fn refunds_and_priority_should_work() {
	new_test_ext().execute_with(|| {
		let author = EVM::find_author();
		let before_tip = EVM::account_basic(&author).0.balance;
		let before_call = EVM::account_basic(&H160::default()).0.balance;
		// We deliberately set a base fee + max tip > max fee.
		// The effective priority tip will be 1GWEI instead 1.5GWEI:
		// 		(max_fee_per_gas - base_fee).min(max_priority_fee)
		//		(2 - 1).min(1.5)
		let tip = U256::from(1_500_000_000);
		let max_fee_per_gas = U256::from(2_000_000_000);
		let used_gas = U256::from(21_000);
		let _ = EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1),
			1000000,
			max_fee_per_gas,
			Some(tip),
			None,
			Vec::new(),
		);
		let (base_fee, _) = <Test as Config>::FeeCalculator::min_gas_price();
		let actual_tip = (max_fee_per_gas - base_fee).min(tip) * used_gas;
		let total_cost = (used_gas * base_fee) + actual_tip + U256::from(1);
		let after_call = EVM::account_basic(&H160::default()).0.balance;
		// The tip is deducted but never refunded to the caller.
		assert_eq!(after_call, before_call - total_cost);

		let after_tip = EVM::account_basic(&author).0.balance;
		assert_eq!(after_tip, (before_tip + actual_tip));
	});
}

#[test]
fn call_should_fail_with_priority_greater_than_max_fee() {
	new_test_ext().execute_with(|| {
		// Max priority greater than max fee should fail.
		let tip: u128 = 1_100_000_000;
		let result = EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1),
			1000000,
			U256::from(1_000_000_000),
			Some(U256::from(tip)),
			None,
			Vec::new(),
		);
		assert!(result.is_err());
		// Some used weight is returned as part of the error.
		assert_eq!(
			result.unwrap_err().post_info.actual_weight,
			Some(Weight::from_parts(7, 0))
		);
	});
}

#[test]
fn call_should_succeed_with_priority_equal_to_max_fee() {
	new_test_ext().execute_with(|| {
		let tip: u128 = 1_000_000_000;
		// Mimics the input for pre-eip-1559 transaction types where `gas_price`
		// is used for both `max_fee_per_gas` and `max_priority_fee_per_gas`.
		let result = EVM::call(
			RuntimeOrigin::root(),
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1),
			1000000,
			U256::from(1_000_000_000),
			Some(U256::from(tip)),
			None,
			Vec::new(),
		);
		assert!(result.is_ok());
	});
}

#[test]
fn handle_sufficient_reference() {
	new_test_ext().execute_with(|| {
		let addr = H160::from_str("1230000000000000000000000000000000000001").unwrap();
		let addr_2 = H160::from_str("1234000000000000000000000000000000000001").unwrap();
		let substrate_addr: <Test as frame_system::Config>::AccountId =
			<Test as Config>::AddressMapping::into_account_id(addr);
		let substrate_addr_2: <Test as frame_system::Config>::AccountId =
			<Test as Config>::AddressMapping::into_account_id(addr_2);

		// Sufficients should increase when creating EVM accounts.
		<crate::AccountCodes<Test>>::insert(addr, vec![0]);
		let account = frame_system::Account::<Test>::get(substrate_addr);
		// Using storage is not correct as it leads to a sufficient reference mismatch.
		assert_eq!(account.sufficients, 0);

		// Using the create / remove account functions is the correct way to handle it.
		EVM::create_account(addr_2, vec![1, 2, 3]);
		let account_2 = frame_system::Account::<Test>::get(substrate_addr_2);
		// We increased the sufficient reference by 1.
		assert_eq!(account_2.sufficients, 1);
		EVM::remove_account(&addr_2);
		let account_2 = frame_system::Account::<Test>::get(substrate_addr_2);
		assert_eq!(account_2.sufficients, 1);
	});
}

#[test]
fn runner_non_transactional_calls_with_non_balance_accounts_is_ok_without_gas_price() {
	// Expect to skip checks for gas price and account balance when both:
	//	- The call is non transactional (`is_transactional == false`).
	//	- The `max_fee_per_gas` is None.
	new_test_ext().execute_with(|| {
		let non_balance_account =
			H160::from_str("7700000000000000000000000000000000000001").unwrap();
		assert_eq!(
			EVM::account_basic(&non_balance_account).0.balance,
			U256::zero()
		);
		let _ = <Test as Config>::Runner::call(
			non_balance_account,
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1u32),
			1000000,
			None,
			None,
			None,
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			&<Test as Config>::config().clone(),
		)
		.expect("Non transactional call succeeds");
		assert_eq!(
			EVM::account_basic(&non_balance_account).0.balance,
			U256::zero()
		);
	});
}

#[test]
fn runner_non_transactional_calls_with_non_balance_accounts_is_err_with_gas_price() {
	// In non transactional calls where `Some(gas_price)` is defined, expect it to be
	// checked against the `BaseFee`, and expect the account to have enough balance
	// to pay for the call.
	new_test_ext().execute_with(|| {
		let non_balance_account =
			H160::from_str("7700000000000000000000000000000000000001").unwrap();
		assert_eq!(
			EVM::account_basic(&non_balance_account).0.balance,
			U256::zero()
		);
		let res = <Test as Config>::Runner::call(
			non_balance_account,
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1u32),
			1000000,
			Some(U256::from(1_000_000_000)),
			None,
			None,
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			&<Test as Config>::config().clone(),
		);
		assert!(res.is_err());
	});
}

#[test]
fn runner_transactional_call_with_zero_gas_price_fails() {
	// Transactional calls are rejected when `max_fee_per_gas == None`.
	new_test_ext().execute_with(|| {
		let res = <Test as Config>::Runner::call(
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1u32),
			1000000,
			None,
			None,
			None,
			Vec::new(),
			true, // transactional
			true, // must be validated
			None,
			&<Test as Config>::config().clone(),
		);
		assert!(res.is_err());
	});
}

#[test]
fn runner_max_fee_per_gas_gte_max_priority_fee_per_gas() {
	// Transactional and non transactional calls enforce `max_fee_per_gas >= max_priority_fee_per_gas`.
	new_test_ext().execute_with(|| {
		let res = <Test as Config>::Runner::call(
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1u32),
			1000000,
			Some(U256::from(1_000_000_000)),
			Some(U256::from(2_000_000_000)),
			None,
			Vec::new(),
			true, // transactional
			true, // must be validated
			None,
			&<Test as Config>::config().clone(),
		);
		assert!(res.is_err());
		let res = <Test as Config>::Runner::call(
			H160::default(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1u32),
			1000000,
			Some(U256::from(1_000_000_000)),
			Some(U256::from(2_000_000_000)),
			None,
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			&<Test as Config>::config().clone(),
		);
		assert!(res.is_err());
	});
}

#[test]
fn eip3607_transaction_from_contract() {
	new_test_ext().execute_with(|| {
		// external transaction
		match <Test as Config>::Runner::call(
			// Contract address.
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1u32),
			1000000,
			None,
			None,
			None,
			Vec::new(),
			true,  // transactional
			false, // not sure be validated
			None,
			&<Test as Config>::config().clone(),
		) {
			Err(RunnerError {
				error: Error::TransactionMustComeFromEOA,
				..
			}) => (),
			_ => panic!("Should have failed"),
		}

		// internal call
		assert!(<Test as Config>::Runner::call(
			// Contract address.
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			H160::from_str("1000000000000000000000000000000000000001").unwrap(),
			Vec::new(),
			U256::from(1u32),
			1000000,
			None,
			None,
			None,
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			&<Test as Config>::config().clone(),
		)
		.is_ok());
	});
}

#[test]
fn metadata_code_gets_cached() {
	new_test_ext().execute_with(|| {
		let address = H160::repeat_byte(0xaa);

		crate::Pallet::<Test>::create_account(address, b"Exemple".to_vec());

		let metadata = crate::Pallet::<Test>::account_code_metadata(address);
		assert_eq!(metadata.size, 7);
		assert_eq!(
			metadata.hash,
			hex_literal::hex!("e8396a990fe08f2402e64a00647e41dadf360ba078a59ba79f55e876e67ed4bc")
				.into()
		);

		let metadata2 = <AccountCodesMetadata<Test>>::get(address).expect("to have metadata set");
		assert_eq!(metadata, metadata2);
	});
}

#[test]
fn metadata_empty_dont_code_gets_cached() {
	new_test_ext().execute_with(|| {
		let address = H160::repeat_byte(0xaa);

		let metadata = crate::Pallet::<Test>::account_code_metadata(address);
		assert_eq!(metadata.size, 0);
		assert_eq!(
			metadata.hash,
			hex_literal::hex!("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470")
				.into()
		);

		assert!(<AccountCodesMetadata<Test>>::get(address).is_none());
	});
}

// SPDX-License-Identifier: GPL-3.0
// pragma solidity >=0.8.2 <0.9.0;
// contract ProofTest {
//     uint256 number;
//
//     function set_number(uint num) public {
//         number = num;
//     }
//
//     function get_number() public view returns (uint256) {
//         return number;
//     }
// }

const PROOF_TEST_BYTECODE: &'static str = "6080604052348015600e575f80fd5b506101438061001c5f395ff3fe608060405234801561000f575f80fd5b5060043610610034575f3560e01c8063d6d1ee1414610038578063eeb4e36714610054575b5f80fd5b610052600480360381019061004d91906100ba565b610072565b005b61005c61007b565b60405161006991906100f4565b60405180910390f35b805f8190555050565b5f8054905090565b5f80fd5b5f819050919050565b61009981610087565b81146100a3575f80fd5b50565b5f813590506100b481610090565b92915050565b5f602082840312156100cf576100ce610083565b5b5f6100dc848285016100a6565b91505092915050565b6100ee81610087565b82525050565b5f6020820190506101075f8301846100e5565b9291505056fea26469706673582212201114104d5a56d94d03255e0f9fa699d53db26e355fb37f735caa200d7ce5158e64736f6c634300081a0033";

#[test]
fn proof_size_create_contract() {
	let proof_size =
		|| -> Option<u64> { cumulus_primitives_storage_weight_reclaim::get_proof_size() };

	let mut test_ext_with_recorder = new_text_ext_with_recorder();
	test_ext_with_recorder.execute_with(|| {
		// The initial proof size should be 0
		assert_eq!(proof_size(), Some(0));
		// Read the storage increases the proof size
		EVM::account_basic(&H160::from_str("1000000000000000000000000000000000000002").unwrap());
		assert_eq!(proof_size(), Some(583));
		AccountCodes::<Test>::get(
			&H160::from_str("1000000000000000000000000000000000000001").unwrap(),
		);
		assert_eq!(proof_size(), Some(799));
	});

	test_ext_with_recorder.execute_with(|| {
		let transaction_pov =
			TransactionPov::new(Weight::from_parts(10000000000000, 5000), 100, proof_size());
		let res = <Test as Config>::Runner::create(
			H160::default(),
			hex::decode(PROOF_TEST_BYTECODE).unwrap(),
			U256::zero(),
			10000000,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			true, // transactional|
			true, // must be validated
			Some(transaction_pov),
			&<Test as Config>::config().clone(),
		)
		.expect("create contract failed");
		let contract_addr = res.value;
		assert!(AccountCodes::<Test>::get(contract_addr).len() != 0);
		assert_eq!(proof_size(), Some(1196));
	});
}

#[test]
fn proof_size_create_contract_with_low_proof_limit() {
	let proof_size =
		|| -> Option<u64> { cumulus_primitives_storage_weight_reclaim::get_proof_size() };

	let mut test_ext_with_recorder = new_text_ext_with_recorder();
	test_ext_with_recorder.execute_with(|| {
		// Return error is the maximum proof size is less than the extrinsic length
		let transaction_pov =
			TransactionPov::new(Weight::from_parts(10000000000000, 50), 100, proof_size());
		assert!(<Test as Config>::Runner::create(
			H160::default(),
			hex::decode(PROOF_TEST_BYTECODE).unwrap(),
			U256::zero(),
			10000000,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(transaction_pov),
			&<Test as Config>::config().clone(),
		)
		.is_err());
	});
}

#[test]
fn proof_size_reach_limit() {
	let proof_size =
		|| -> Option<u64> { cumulus_primitives_storage_weight_reclaim::get_proof_size() };

	let mut test_ext_with_recorder = new_text_ext_with_recorder();
	// create contract run out of proof size
	test_ext_with_recorder.execute_with(|| {
		let transaction_pov =
			TransactionPov::new(Weight::from_parts(10000000000000, 101), 100, proof_size());
		let res = <Test as Config>::Runner::create(
			H160::default(),
			hex::decode(PROOF_TEST_BYTECODE).unwrap(),
			U256::zero(),
			10000000,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(transaction_pov),
			&<Test as Config>::config().clone(),
		)
		.expect("create contract failed");
		assert_eq!(res.exit_reason, ExitReason::Error(ExitError::OutOfGas));
		let contract_addr = res.value;
		assert!(AccountCodes::<Test>::get(contract_addr).len() == 0);
	});

	// call contract run out of proof size
	test_ext_with_recorder.execute_with(|| {
		let mut transaction_pov =
			TransactionPov::new(Weight::from_parts(10000000000000, 5000), 100, proof_size());
		let res = <Test as Config>::Runner::create(
			H160::default(),
			hex::decode(PROOF_TEST_BYTECODE).unwrap(),
			U256::zero(),
			10000000,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(transaction_pov),
			&<Test as Config>::config().clone(),
		)
		.expect("create contract failed");
		let contract_addr = res.value;
		assert!(AccountCodes::<Test>::get(contract_addr).len() != 0);

		// set_number(6)
		let calldata = "d6d1ee140000000000000000000000000000000000000000000000000000000000000006";
		transaction_pov.weight_limit = Weight::from_parts(10000000000000, 99);
		let res = <Test as Config>::Runner::call(
			H160::default(),
			contract_addr,
			hex::decode(calldata).unwrap(),
			U256::zero(),
			10000000,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			true,  // transactional
			false, // must be validated
			Some(transaction_pov),
			&<Test as Config>::config().clone(),
		)
		.expect("call contract failed");
		assert_eq!(res.exit_reason, ExitReason::Error(ExitError::OutOfGas));

		// get_number()
		let calldata = "eeb4e367";
		transaction_pov.weight_limit = Weight::from_parts(10000000000000, 50000);
		let res = <Test as Config>::Runner::call(
			H160::default(),
			contract_addr,
			hex::decode(calldata).unwrap(),
			U256::zero(),
			10000000,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			true,  // transactional
			false, // must be validated
			Some(transaction_pov),
			&<Test as Config>::config().clone(),
		)
		.expect("call contract failed");
		assert_eq!(U256::from_big_endian(&res.value), U256::from(0));
	});
}

#[test]
fn proof_size_reach_limit_nonce_increase() {
	let proof_size =
		|| -> Option<u64> { cumulus_primitives_storage_weight_reclaim::get_proof_size() };

	let mut test_ext_with_recorder = new_text_ext_with_recorder();
	test_ext_with_recorder.execute_with(|| {
		let original_nonce = EVM::account_basic(&H160::default()).0.nonce;
		let transaction_pov =
			TransactionPov::new(Weight::from_parts(10000000000000, 101), 100, proof_size());
		let res = <Test as Config>::Runner::create(
			H160::default(),
			hex::decode(PROOF_TEST_BYTECODE).unwrap(),
			U256::zero(),
			10000000,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(transaction_pov),
			&<Test as Config>::config().clone(),
		)
		.expect("create contract failed");
		assert_eq!(res.exit_reason, ExitReason::Error(ExitError::OutOfGas));
		assert_eq!(
			EVM::account_basic(&H160::default()).0.nonce,
			original_nonce + 1
		);
	});
}
