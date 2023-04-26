// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
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
use fp_ethereum::ValidatedTransaction;
use frame_support::{
	dispatch::{DispatchClass, GetDispatchInfo},
	weights::Weight,
};
use pallet_evm::{AddressMapping, GasWeightMapping};
use scale_codec::Encode;

fn legacy_proof_size_test_callee_create() -> LegacyUnsignedTransaction {
	LegacyUnsignedTransaction {
		nonce: U256::zero(),
		gas_price: U256::from(1),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Create,
		value: U256::zero(),
		input: hex::decode(PROOF_SIZE_TEST_CALLEE_CONTRACT_BYTECODE.trim_end()).unwrap(),
	}
}

fn legacy_proof_size_test_create() -> LegacyUnsignedTransaction {
	LegacyUnsignedTransaction {
		nonce: U256::zero(),
		gas_price: U256::from(1),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Create,
		value: U256::zero(),
		input: hex::decode(PROOF_SIZE_TEST_CONTRACT_BYTECODE.trim_end()).unwrap(),
	}
}

fn legacy_erc20_creation_unsigned_transaction() -> LegacyUnsignedTransaction {
	LegacyUnsignedTransaction {
		nonce: U256::zero(),
		gas_price: U256::from(1),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Create,
		value: U256::zero(),
		input: hex::decode(ERC20_CONTRACT_BYTECODE.trim_end()).unwrap(),
	}
}

fn legacy_erc20_creation_transaction(account: &AccountInfo) -> Transaction {
	legacy_erc20_creation_unsigned_transaction().sign(&account.private_key)
}

#[test]
fn transaction_should_increment_nonce() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);
		assert_ok!(Ethereum::execute(alice.address, &t, None, None,));
		assert_eq!(EVM::account_basic(&alice.address).0.nonce, U256::from(1));
	});
}

#[test]
fn transaction_without_enough_gas_should_not_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut transaction = legacy_erc20_creation_transaction(alice);
		match &mut transaction {
			Transaction::Legacy(t) => t.gas_price = U256::from(11_000_000),
			_ => {}
		}

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<u64, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call.clone()),
		};
		let dispatch_info = extrinsic.get_dispatch_info();

		assert_err!(
			call.validate_self_contained(&source, &dispatch_info, 0)
				.unwrap(),
			InvalidTransaction::Payment
		);
	});
}

#[test]
fn transaction_with_to_low_nonce_should_not_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		// nonce is 0
		let mut transaction = legacy_erc20_creation_unsigned_transaction();
		transaction.nonce = U256::from(1);

		let signed = transaction.sign(&alice.private_key);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<u64, _, SignedExtra, H160> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call.clone()),
		};
		let dispatch_info = extrinsic.get_dispatch_info();

		assert_eq!(
			call.validate_self_contained(&source, &dispatch_info, 0)
				.unwrap(),
			ValidTransactionBuilder::default()
				.and_provides((alice.address, U256::from(1)))
				.priority(0u64)
				.and_requires((alice.address, U256::from(0)))
				.build()
		);

		let t = legacy_erc20_creation_transaction(alice);

		// nonce is 1
		assert_ok!(Ethereum::execute(alice.address, &t, None, None,));

		transaction.nonce = U256::from(0);

		let signed2 = transaction.sign(&alice.private_key);
		let call2 = crate::Call::<Test>::transact {
			transaction: signed2,
		};
		let source2 = call2.check_self_contained().unwrap().unwrap();
		let extrinsic2 = CheckedExtrinsic::<u64, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call2.clone()),
		};

		assert_err!(
			call2
				.validate_self_contained(&source2, &extrinsic2.get_dispatch_info(), 0)
				.unwrap(),
			InvalidTransaction::Stale
		);
	});
}

#[test]
fn transaction_with_to_hight_nonce_should_fail_in_block() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut transaction = legacy_erc20_creation_unsigned_transaction();
		transaction.nonce = U256::one();

		let signed = transaction.sign(&alice.private_key);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call),
		};
		let dispatch_info = extrinsic.get_dispatch_info();
		assert_err!(
			extrinsic.apply::<Test>(&dispatch_info, 0),
			TransactionValidityError::Invalid(InvalidTransaction::Future)
		);
	});
}

#[test]
fn transaction_with_invalid_chain_id_should_fail_in_block() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let transaction =
			legacy_erc20_creation_unsigned_transaction().sign_with_chain_id(&alice.private_key, 1);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call),
		};
		let dispatch_info = extrinsic.get_dispatch_info();
		assert_err!(
			extrinsic.apply::<Test>(&dispatch_info, 0),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				fp_ethereum::TransactionValidationError::InvalidChainId as u8,
			))
		);
	});
}

#[test]
fn contract_constructor_should_get_executed() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];
	let erc20_address = contract_address(alice.address, 0);
	let alice_storage_address = storage_address(alice.address, H256::zero());

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);

		assert_ok!(Ethereum::execute(alice.address, &t, None, None,));
		assert_eq!(
			EVM::account_storages(erc20_address, alice_storage_address),
			H256::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
				.unwrap()
		)
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
			RawOrigin::EthereumTransaction(alice.address).into(),
			legacy_erc20_creation_transaction(alice),
		)
		.expect("Failed to execute transaction");

		// We verify the transaction happened with alice account.
		assert_eq!(
			EVM::account_storages(erc20_address, alice_storage_address),
			H256::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
				.unwrap()
		)
	});
}

#[test]
fn contract_should_be_created_at_given_address() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let erc20_address = contract_address(alice.address, 0);

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);
		assert_ok!(Ethereum::execute(alice.address, &t, None, None,));
		assert_ne!(EVM::account_codes(erc20_address).len(), 0);
	});
}

#[test]
fn transaction_should_generate_correct_gas_used() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let expected_gas = U256::from(893928);

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);
		let (_, _, info) = Ethereum::execute(alice.address, &t, None, None,).unwrap();

		match info {
			CallOrCreateInfo::Create(info) => {
				assert_eq!(info.used_gas, expected_gas);
			}
			CallOrCreateInfo::Call(_) => panic!("expected create info"),
		}
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
		let t = LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Create,
			value: U256::zero(),
			input: hex::decode(contract).unwrap(),
		}
		.sign(&alice.private_key);
		assert_ok!(Ethereum::execute(alice.address, &t, None, None,));

		let contract_address = hex::decode("32dcab0ef3fb2de2fce1d2e0799d36239671f04a").unwrap();
		let foo = hex::decode("c2985578").unwrap();
		let bar = hex::decode("febb0f7e").unwrap();

		let t2 = LegacyUnsignedTransaction {
			nonce: U256::from(1),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: foo,
		}
		.sign(&alice.private_key);

		// calling foo will succeed
		let (_, _, info) = Ethereum::execute(alice.address, &t2, None, None,).unwrap();

		match info {
			CallOrCreateInfo::Call(info) => {
				assert_eq!(
					hex::encode(info.value),
					"0000000000000000000000000000000000000000000000000000000000000001"
				);
			}
			CallOrCreateInfo::Create(_) => panic!("expected call info"),
		}

		let t3 = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: bar,
		}
		.sign(&alice.private_key);

		// calling should always succeed even if the inner EVM execution fails.
		Ethereum::execute(alice.address, &t3, None, None,).ok().unwrap();
	});
}

#[test]
fn self_contained_transaction_with_extra_gas_should_adjust_weight_with_post_dispatch() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];
	let base_extrinsic_weight = frame_system::limits::BlockWeights::with_sensible_defaults(
		Weight::from_ref_time(2000000000000).set_proof_size(u64::MAX),
		sp_runtime::Perbill::from_percent(75),
	)
	.per_class
	.get(DispatchClass::Normal)
	.base_extrinsic;

	ext.execute_with(|| {
		let mut transaction = legacy_erc20_creation_unsigned_transaction();
		transaction.gas_limit = 9_000_000.into();
		let signed = transaction.sign(&alice.private_key);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, frame_system::CheckWeight<Test>, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call),
		};
		let dispatch_info = extrinsic.get_dispatch_info();
		let post_dispatch_weight = extrinsic
			.apply::<Test>(&dispatch_info, 0)
			.unwrap()
			.unwrap()
			.actual_weight
			.unwrap();

		let expected_weight = base_extrinsic_weight.saturating_add(post_dispatch_weight);
		let actual_weight =
			*frame_system::Pallet::<Test>::block_weight().get(DispatchClass::Normal);
		assert_eq!(
			expected_weight,
			actual_weight,
			"the block weight was unexpected, excess '{}'",
			actual_weight - expected_weight
		);
	});
}

#[test]
fn validated_transaction_apply_zero_gas_price_works() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 1_000);
	let alice = &pairs[0];
	let bob = &pairs[1];
	let substrate_alice =
		<Test as pallet_evm::Config>::AddressMapping::into_account_id(alice.address);
	let substrate_bob = <Test as pallet_evm::Config>::AddressMapping::into_account_id(bob.address);

	ext.execute_with(|| {
		let transaction = LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::zero(),
			gas_limit: U256::from(21_000),
			action: ethereum::TransactionAction::Call(bob.address),
			value: U256::from(100),
			input: Default::default(),
		}
		.sign(&alice.private_key);

		assert_ok!(crate::ValidatedTransaction::<Test>::apply(
			alice.address,
			transaction,
			None
		));
		// Alice didn't pay fees, transfer 100 to Bob.
		assert_eq!(Balances::free_balance(&substrate_alice), 900);
		// Bob received 100 from Alice.
		assert_eq!(Balances::free_balance(&substrate_bob), 1_100);
	});
}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn proof_size_create_accounting_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut callee = legacy_proof_size_test_callee_create();
		let gas_limit: u64 = 1_000_000;
		callee.gas_limit = U256::from(gas_limit);
		let transaction = callee.sign(&alice.private_key);

		let transaction_len = transaction.encode().len() + 1 + 1;

		let weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);
		let result = Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			transaction,
			weight_limit
		)
		.expect("Failed to execute transaction");

		let actual_weight = result.actual_weight.expect("some weight");
		assert_eq!(transaction_len as u64, actual_weight.proof_size());
	});

}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn proof_size_subcall_accounting_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let callee_contract_address = contract_address(alice.address, 0);
	let proof_size_test_contract_address = contract_address(alice.address, 1);

	ext.execute_with(|| {
		// Create callee contract A
		let callee = legacy_proof_size_test_callee_create()
			.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			callee,
		)
		.expect("Failed to execute transaction");

		// Create proof size test contract B
		let mut proof_size_test = legacy_proof_size_test_create();
		proof_size_test.nonce = U256::from(1);
		let proof_size_test = proof_size_test.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			proof_size_test,
		)
		.expect("Failed to execute transaction");

		// Call B, that calls A, with weight limit
		// selector for ProofSizeTest::test_call function..
		let mut call_data: String = "c6d6f606000000000000000000000000".to_owned();
		// ..encode the callee address argument
		call_data.push_str(&format!("{:x}", callee_contract_address));
		let mut subcall = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(proof_size_test_contract_address),
			value: U256::zero(),
			input: hex::decode(&call_data).unwrap(),
		};

		let gas_limit: u64 = 1_000_000;
		subcall.gas_limit = U256::from(gas_limit);

		let weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		let subcall = subcall.sign(&alice.private_key);

		// Expected proof size
		let transaction_len = subcall.encode().len() + 1 + 1;
		let read_account_metadata = ACCOUNT_CODES_METADATA_PROOF_SIZE as usize;
		let reading_contract_len = EVM::account_codes(callee_contract_address).len();
		
		let expected_proof_size = transaction_len + read_account_metadata + reading_contract_len;

		// Execute
		let result = Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			subcall,
			weight_limit,
		)
		.expect("Failed to execute transaction");

		// Expect recorded proof size to be equal the expected proof size
		let actual_weight = result.actual_weight.expect("some weight");
		assert_eq!(expected_proof_size as u64, actual_weight.proof_size());
	});
}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn proof_size_balance_accounting_works() {
	let (pairs, mut ext) = new_test_ext(2);
	let alice = &pairs[0];
	let bob = &pairs[1];

	let proof_size_test_contract_address = contract_address(alice.address, 0);

	ext.execute_with(|| {
		// Create proof size test contract
		let proof_size_test = legacy_proof_size_test_create()
			.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			proof_size_test,
		)
		.expect("Failed to execute transaction");

		// selector for ProofSizeTest::balance function..
		let mut call_data: String = "35f56c3b000000000000000000000000".to_owned();
		// ..encode bobs address
		call_data.push_str(&format!("{:x}", bob.address));
		let mut balance = LegacyUnsignedTransaction {
			nonce: U256::from(1),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(proof_size_test_contract_address),
			value: U256::zero(),
			input: hex::decode(&call_data).unwrap(),
		};

		let gas_limit: u64 = 1_000_000;
		balance.gas_limit = U256::from(gas_limit);

		let weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		let balance = balance.sign(&alice.private_key);

		// Expected proof size, the transaction length + an account basic cold read.
		let transaction_len = balance.encode().len() + 1 + 1;
		// Contract makes two account reads - cold, then warm -, only cold reads record proof size.
		let read_account_basic = ACCOUNT_BASIC_PROOF_SIZE as usize; // wip move this to constants, this is the value for reading a maybe cached account code.
		
		let expected_proof_size = transaction_len + read_account_basic;

		// Execute
		let result = Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			balance,
			weight_limit,
		)
		.expect("Failed to execute transaction");

		// Expect recorded proof size to be equal the expected proof size
		let actual_weight = result.actual_weight.expect("some weight");
		assert_eq!(expected_proof_size as u64, actual_weight.proof_size());
	});
}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn proof_size_sload_accounting_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let proof_size_test_contract_address = contract_address(alice.address, 0);

	ext.execute_with(|| {
		// Create proof size test contract
		let proof_size_test = legacy_proof_size_test_create()
			.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			proof_size_test,
		)
		.expect("Failed to execute transaction");

		// selector for ProofSizeTest::test_sload function..
		let call_data: String = "e27a0ecd".to_owned();
		let mut balance = LegacyUnsignedTransaction {
			nonce: U256::from(0),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(proof_size_test_contract_address),
			value: U256::zero(),
			input: hex::decode(&call_data).unwrap(),
		};

		let gas_limit: u64 = 1_000_000;
		balance.gas_limit = U256::from(gas_limit);

		let weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		let balance = balance.sign(&alice.private_key);

		// Expected proof size, the transaction length + an account basic cold read.
		let transaction_len = balance.encode().len() + 1 + 1;
		// Contract does two sloads - cold, then warm -, only cold reads record proof size.
		let sload_cost = ACCOUNT_STORAGE_PROOF_SIZE as usize; // wip move this to constants, this is the value for reading a maybe cached account code.
		
		let expected_proof_size = transaction_len + sload_cost;

		// Execute
		let result = Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			balance,
			weight_limit,
		)
		.expect("Failed to execute transaction");

		// Expect recorded proof size to be equal the expected proof size
		let actual_weight = result.actual_weight.expect("some weight");
		assert_eq!(expected_proof_size as u64, actual_weight.proof_size());
	});
}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn proof_size_sstore_accounting_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let proof_size_test_contract_address = contract_address(alice.address, 0);

	ext.execute_with(|| {
		// Create proof size test contract
		let proof_size_test = legacy_proof_size_test_create()
			.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			proof_size_test,
		)
		.expect("Failed to execute transaction");

		// selector for ProofSizeTest::test_sstore function..
		let call_data: String = "4f3080a9".to_owned();
		let mut balance = LegacyUnsignedTransaction {
			nonce: U256::from(0),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(proof_size_test_contract_address),
			value: U256::zero(),
			input: hex::decode(&call_data).unwrap(),
		};

		let gas_limit: u64 = 1_000_000;
		balance.gas_limit = U256::from(gas_limit);

		let weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		let balance = balance.sign(&alice.private_key);

		// Expected proof size, the transaction length + an account basic cold read.
		let transaction_len = balance.encode().len() + 1 + 1;
		// Contract does two sstore - cold, then warm -, only cold writes record proof size.
		let sstore_cost = HASH_PROOF_SIZE as usize; // wip move this to constants, this is the value for reading a maybe cached account code.
		
		let expected_proof_size = transaction_len + sstore_cost;

		// Execute
		let result = Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			balance,
			weight_limit,
		)
		.expect("Failed to execute transaction");

		// Expect recorded proof size to be equal the expected proof size
		let actual_weight = result.actual_weight.expect("some weight");
		assert_eq!(expected_proof_size as u64, actual_weight.proof_size());
	});
}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn proof_size_oog_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let callee_contract_address = contract_address(alice.address, 0);
	let proof_size_test_contract_address = contract_address(alice.address, 1);

	ext.execute_with(|| {
		// Create callee contract A
		let callee = legacy_proof_size_test_callee_create()
			.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			callee,
		)
		.expect("Failed to execute transaction");

		// Create proof size test contract B
		let mut proof_size_test = legacy_proof_size_test_create();
		proof_size_test.nonce = U256::from(1);
		let proof_size_test = proof_size_test.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			proof_size_test,
		)
		.expect("Failed to execute transaction");

		// Call B, that calls A infinitely, with weight limit
		// selector for ProofSizeTest::test_oog function..
		let call_data: String = "944ddc62".to_owned();
		let mut subcall = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(proof_size_test_contract_address),
			value: U256::zero(),
			input: hex::decode(&call_data).unwrap(),
		};

		let gas_limit: u64 = 1_000_000;
		subcall.gas_limit = U256::from(gas_limit);

		let mut weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		// Artifically set a lower proof size limit so we OOG this instead gas.
		*weight_limit.proof_size_mut() = weight_limit.proof_size() / 2;

		let subcall = subcall.sign(&alice.private_key);

		let transaction_len = (subcall.encode().len() + 1 + 1) as u64;
		// Transaction len is recorded initially and thus not available in the evm execution.
		// Find how many random balance reads can we do with the available proof size.
		let available_proof_size = weight_limit.proof_size() - transaction_len;
		let number_balance_reads =  available_proof_size.saturating_div(ACCOUNT_BASIC_PROOF_SIZE);
		// The actual proof size consumed by those balance reads, plus the transaction_len.
		let expected_total_proof_size =  (number_balance_reads * ACCOUNT_BASIC_PROOF_SIZE) + transaction_len;
		
		// Execute
		let result = Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			subcall,
			weight_limit,
		)
		.expect("Failed to execute transaction");

		// Expect recorded proof size to be equal the expected proof size
		let actual_weight = result.actual_weight.expect("some weight");
		assert_eq!(expected_total_proof_size, actual_weight.proof_size());
	});
}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn proof_size_weight_limit_validation_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let proof_size_test_contract_address = contract_address(alice.address, 1);

	ext.execute_with(|| {
		// any selector
		let call_data: String = "944ddc62".to_owned();
		let mut subcall = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(proof_size_test_contract_address),
			value: U256::zero(),
			input: hex::decode(&call_data).unwrap(),
		};

		let gas_limit: u64 = 1_000_000;
		subcall.gas_limit = U256::from(gas_limit);

		let mut weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		let subcall = subcall.sign(&alice.private_key);
		let transaction_len = (subcall.encode().len() + 1 + 1) as u64;

		// Artifically set a failing proof size limit
		*weight_limit.proof_size_mut() = transaction_len - 1;
		
		// Execute
		assert_eq!(Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			subcall,
			weight_limit,
		), Err(frame_support::dispatch::DispatchErrorWithPostInfo {
			post_info: frame_support::dispatch::PostDispatchInfo {
				actual_weight: None,
				pays_fee: frame_support::dispatch::Pays::No,
			},
			error: frame_support::dispatch::DispatchError::Exhausted,
		}));
	});
}

#[cfg(feature = "evm-with-weight-limit")]
#[test]
fn uncached_account_code_proof_size_accounting_works() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let callee_contract_address = contract_address(alice.address, 0);
	let proof_size_test_contract_address = contract_address(alice.address, 1);

	ext.execute_with(|| {
		// Create callee contract A
		let callee = legacy_proof_size_test_callee_create()
			.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			callee,
		)
		.expect("Failed to execute transaction");

		// Assert callee contract code hash and size are cached
		let pallet_evm::CodeMetadata { size, .. } = <pallet_evm::AccountCodesMetadata<Test>>::get(callee_contract_address)
			.expect("contract code hash and size are cached");

		// Remove callee cache
		<pallet_evm::AccountCodesMetadata<Test>>::remove(callee_contract_address);

		// Create proof size test contract B
		let mut proof_size_test = legacy_proof_size_test_create();
		proof_size_test.nonce = U256::from(1);
		let proof_size_test = proof_size_test.sign(&alice.private_key);
		let _ = Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			proof_size_test,
		)
		.expect("Failed to execute transaction");

		// Call B, that calls A, with weight limit
		// selector for ProofSizeTest::test_call function..
		let mut call_data: String = "c6d6f606000000000000000000000000".to_owned();
		// ..encode the callee address argument
		call_data.push_str(&format!("{:x}", callee_contract_address));
		let mut subcall = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(proof_size_test_contract_address),
			value: U256::zero(),
			input: hex::decode(&call_data).unwrap(),
		};

		let gas_limit: u64 = 1_000_000;
		subcall.gas_limit = U256::from(gas_limit);

		let weight_limit = <Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		let subcall = subcall.sign(&alice.private_key);

		// Expected proof size
		let transaction_len = subcall.encode().len() + 1 + 1;
		let read_account_metadata = ACCOUNT_CODES_METADATA_PROOF_SIZE as usize;
		let reading_contract_len = EVM::account_codes(callee_contract_address).len();
		// In addition, callee code size is unchached and thus included in the pov
		let expected_proof_size = transaction_len + read_account_metadata + reading_contract_len + size as usize;

		// Execute
		let result = Ethereum::transact_with_weight_limit(
			RawOrigin::EthereumTransaction(alice.address).into(),
			subcall,
			weight_limit,
		)
		.expect("Failed to execute transaction");

		// Expect recorded proof size to be equal the expected proof size
		let actual_weight = result.actual_weight.expect("some weight");
		assert_eq!(expected_proof_size as u64, actual_weight.proof_size());
	});
}