// This file is part of Substrate.

// Copyright (C) 2019-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Consensus extension module tests for BABE consensus.

use super::*;
use mock::*;
use rustc_hex::{FromHex, ToHex};
use std::str::FromStr;
use ethereum::TransactionSignature;
use frame_support::{
	assert_noop, assert_err, assert_ok,
	unsigned::ValidateUnsigned,
};
use sp_runtime::transaction_validity::{TransactionSource, InvalidTransaction};

// This ERC-20 contract mints the maximum amount of tokens to the contract creator.
// pragma solidity ^0.5.0;
// import "https://github.com/OpenZeppelin/openzeppelin-contracts/blob/v2.5.1/contracts/token/ERC20/ERC20.sol";
// contract MyToken is ERC20 {
//	 constructor() public { _mint(msg.sender, 2**256 - 1); }
// }
const ERC20_CONTRACT_BYTECODE: &str = include_str!("../res/erc20_contract_bytecode.txt");

fn default_erc20_creation_unsigned_transaction() -> UnsignedTransaction {
	UnsignedTransaction {
		nonce: U256::zero(),
		gas_price: U256::from(1),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Create,
		value: U256::zero(),
		input: FromHex::from_hex(ERC20_CONTRACT_BYTECODE).unwrap(),
	}
}

fn default_erc20_creation_transaction(account: &AccountInfo) -> Transaction {
	default_erc20_creation_unsigned_transaction().sign(&account.private_key)
}

#[test]
fn transaction_should_increment_nonce() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		assert_ok!(Ethereum::execute(
			alice.address,
			default_erc20_creation_transaction(alice),
		));
		assert_eq!(Evm::account_basic(&alice.address).nonce, U256::from(1));
	});
}


#[test]
fn transaction_should_be_added_to_pending() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let transaction = default_erc20_creation_transaction(alice);
		assert_ok!(Ethereum::execute(
			alice.address,
			transaction.clone(),
		));
		assert_eq!(Pending::get().len(), 1);
		assert_eq!(Pending::get()[0].0.input, transaction.input);
	});
}


#[test]
fn transaction_without_enough_gas_should_not_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut transaction = default_erc20_creation_transaction(alice);
		transaction.gas_price = U256::from(11_000_000);

		assert_err!(Ethereum::validate_unsigned(TransactionSource::External, &Call::transact(transaction)), InvalidTransaction::Payment);
	});
}

#[test]
fn transaction_with_invalid_nonce_should_not_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		// nonce is 0
		let mut transaction = default_erc20_creation_unsigned_transaction();
		transaction.nonce = U256::from(1);

		let signed = transaction.sign(&alice.private_key);

		assert_eq!(
			Ethereum::validate_unsigned(TransactionSource::External, &Call::transact(signed)),
			ValidTransaction::with_tag_prefix("Ethereum")
				.and_provides((&alice.address, U256::from(1)))
				.and_requires((&alice.address, U256::from(0)))
				.build()
		);

		// nonce is 1
		assert_ok!(Ethereum::execute(
			alice.address,
			default_erc20_creation_transaction(alice),
		));

		transaction.nonce = U256::from(0);

		let signed2 = transaction.sign(&alice.private_key);

		assert_err!(Ethereum::validate_unsigned(TransactionSource::External, &Call::transact(signed2)), InvalidTransaction::Stale);
	});
}

#[test]
fn contract_constructor_should_get_executed() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];
	let erc20_address = contract_address(alice.address, 0);
	let alice_storage_address = storage_address(alice.address, H256::zero());

	ext.execute_with(|| {
		assert_ok!(Ethereum::execute(
			alice.address,
			default_erc20_creation_transaction(alice),
		));
		assert_eq!(Evm::account_storages(
			erc20_address, alice_storage_address
		), H256::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())
	});
}

#[test]
fn source_should_be_derived_from_signature() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let erc20_address = contract_address(alice.address, 0);
	let alice_storage_address = storage_address(alice.address, H256::zero());

	ext.execute_with(|| {
		Ethereum::transact(
			Origin::none(),
			default_erc20_creation_transaction(alice),
		).expect("Failed to execute transaction");

		// We verify the transaction happened with alice account.
		assert_eq!(Evm::account_storages(
			erc20_address, alice_storage_address
		), H256::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())

	});
}

#[test]
fn invalid_signature_should_be_ignored() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let mut transaction = default_erc20_creation_transaction(alice);
	transaction.signature = TransactionSignature::new(0x78, H256::from_slice(&[55u8;32]), H256::from_slice(&[55u8;32])).unwrap();
	ext.execute_with(|| {
		assert_noop!(Ethereum::transact(
			Origin::none(),
			transaction,
		), Error::<Test>::InvalidSignature);
	});
}

#[test]
fn contract_should_be_created_at_given_address() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let erc20_address = contract_address(alice.address, 0);

	ext.execute_with(|| {
		assert_ok!(Ethereum::execute(
			alice.address,
			default_erc20_creation_transaction(alice),
		));
		assert_ne!(Evm::account_codes(erc20_address).len(), 0);
	});
}

#[test]
fn transaction_should_generate_correct_gas_used() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let expected_gas = U256::from(891328);

	ext.execute_with(|| {
		assert_ok!(Ethereum::execute(
			alice.address,
			default_erc20_creation_transaction(alice),
		));
		assert_eq!(Pending::get()[0].2.used_gas, expected_gas);
	});
}

#[test]
fn call_should_handle_errors() {
	// 	pragma solidity ^0.6.6;
	// 	contract Test {
	// 		function foo() external pure returns (bool) {
	// 			return true;
	// 		}
	// 		function bar() external pure {
	// 			require(false, "error_msg");
	// 		}
	// 	}
	let contract: &str = "608060405234801561001057600080fd5b50610113806100206000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c8063c2985578146037578063febb0f7e146057575b600080fd5b603d605f565b604051808215151515815260200191505060405180910390f35b605d6068565b005b60006001905090565b600060db576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260098152602001807f6572726f725f6d7367000000000000000000000000000000000000000000000081525060200191505060405180910390fd5b56fea2646970667358221220fde68a3968e0e99b16fabf9b2997a78218b32214031f8e07e2c502daf603a69e64736f6c63430006060033";

	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		assert_ok!(Ethereum::execute(
			alice.address,
			UnsignedTransaction {
				nonce: U256::zero(),
				gas_price: U256::from(1),
				gas_limit: U256::from(0x100000),
				action: ethereum::TransactionAction::Create,
				value: U256::zero(),
				input: FromHex::from_hex(contract).unwrap(),
			}.sign(&alice.private_key),
		));

		let contract_address: Vec<u8> = FromHex::from_hex("32dcab0ef3fb2de2fce1d2e0799d36239671f04a").unwrap();
		let foo: Vec<u8> = FromHex::from_hex("c2985578").unwrap();
		let bar: Vec<u8> = FromHex::from_hex("febb0f7e").unwrap();

		// calling foo will succeed
		let (returned, _) = Ethereum::call(
			alice.address,
			foo,
			U256::zero(),
			1048576,
			U256::from(1),
			None,
			TransactionAction::Call(H160::from_slice(&contract_address)),
		).unwrap();
		assert_eq!(returned.to_hex::<String>(), "0000000000000000000000000000000000000000000000000000000000000001".to_owned());

		// calling bar will revert
		let (error, returned) = Ethereum::call(
			alice.address,
			bar,
			U256::zero(),
			1048576,
			U256::from(1),
			None,
			TransactionAction::Call(H160::from_slice(&contract_address))
		).err().unwrap();
		assert_eq!(error, Error::<Test>::Reverted.into());
		assert_eq!(returned.to_hex::<String>(), "08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000096572726f725f6d73670000000000000000000000000000000000000000000000".to_owned());
	});
}
