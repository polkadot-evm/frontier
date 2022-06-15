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
use fp_xcm::{
	AutoEthereumXcmFee, EthereumXcmFee, EthereumXcmTransaction, EthereumXcmTransactionV1,
};
use frame_support::{
	assert_noop,
	weights::{Pays, PostDispatchInfo},
};
use sp_runtime::{DispatchError, DispatchErrorWithPostInfo};

// 	pragma solidity ^0.6.6;
// 	contract Test {
// 		function foo() external pure returns (bool) {
// 			return true;
// 		}
// 		function bar() external pure {
// 			require(false, "error_msg");
// 		}
// 	}
const CONTRACT: &str = "608060405234801561001057600080fd5b50610113806100206000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c8063c2985578146037578063febb0f7e146057575b600080fd5b603d605f565b604051808215151515815260200191505060405180910390f35b605d6068565b005b60006001905090565b600060db576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260098152602001807f6572726f725f6d7367000000000000000000000000000000000000000000000081525060200191505060405180910390fd5b56fea2646970667358221220fde68a3968e0e99b16fabf9b2997a78218b32214031f8e07e2c502daf603a69e64736f6c63430006060033";

fn eip1559_erc20_creation_unsigned_transaction() -> EIP1559UnsignedTransaction {
	EIP1559UnsignedTransaction {
		nonce: U256::zero(),
		max_priority_fee_per_gas: U256::from(1),
		max_fee_per_gas: U256::from(1),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Create,
		value: U256::zero(),
		input: hex::decode(ERC20_CONTRACT_BYTECODE.trim_end()).unwrap(),
	}
}

fn xcm_evm_transfer_eip_1559_transaction(destination: H160, value: U256) -> EthereumXcmTransaction {
	EthereumXcmTransaction::V1(EthereumXcmTransactionV1 {
		fee_payment: EthereumXcmFee::Auto(AutoEthereumXcmFee::Low),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Call(destination),
		value,
		input: vec![],
		access_list: None
	})
}

fn xcm_evm_call_eip_1559_transaction(destination: H160, input: Vec<u8>) -> EthereumXcmTransaction {
	EthereumXcmTransaction::V1(EthereumXcmTransactionV1 {
		fee_payment: EthereumXcmFee::Auto(AutoEthereumXcmFee::Low),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Call(destination),
		value: U256::zero(),
		input,
		access_list: None,
	})
}

fn xcm_erc20_creation_eip_1559_transaction() -> EthereumXcmTransaction {
	EthereumXcmTransaction::V1(EthereumXcmTransactionV1 {
		fee_payment: EthereumXcmFee::Auto(AutoEthereumXcmFee::Low),

		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Create,
		value: U256::zero(),
		input: hex::decode(ERC20_CONTRACT_BYTECODE.trim_end()).unwrap(),
		access_list: None,
	})
}

fn eip1559_erc20_creation_transaction(account: &AccountInfo) -> Transaction {
	eip1559_erc20_creation_unsigned_transaction().sign(&account.private_key, None)
}

#[test]
fn transaction_should_increment_nonce() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = eip1559_erc20_creation_transaction(alice);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));
		assert_eq!(EVM::account_basic(&alice.address).0.nonce, U256::from(1));
	});
}

#[test]
fn transaction_without_enough_gas_should_not_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut transaction = eip1559_erc20_creation_transaction(alice);
		match &mut transaction {
			Transaction::EIP1559(t) => t.max_fee_per_gas = U256::from(11_000_000),
			_ => {}
		}

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<u64, crate::mock::Call, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: Call::Ethereum(call.clone()),
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
		let mut transaction = eip1559_erc20_creation_unsigned_transaction();
		transaction.nonce = U256::from(1);
		let signed = transaction.sign(&alice.private_key, None);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<u64, crate::mock::Call, SignedExtra, H160> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: Call::Ethereum(call.clone()),
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

		let t = eip1559_erc20_creation_transaction(alice);

		// nonce is 1
		assert_ok!(Ethereum::execute(alice.address, &t, None,));
		transaction.nonce = U256::from(0);

		let signed2 = transaction.sign(&alice.private_key, None);
		let call2 = crate::Call::<Test>::transact {
			transaction: signed2,
		};
		let source2 = call2.check_self_contained().unwrap().unwrap();
		let extrinsic2 = CheckedExtrinsic::<u64, crate::mock::Call, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: Call::Ethereum(call2.clone()),
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
		let mut transaction = eip1559_erc20_creation_unsigned_transaction();
		transaction.nonce = U256::one();

		let signed = transaction.sign(&alice.private_key, None);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: Call::Ethereum(call),
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
			eip1559_erc20_creation_unsigned_transaction().sign(&alice.private_key, Some(1));

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: Call::Ethereum(call),
		};
		let dispatch_info = extrinsic.get_dispatch_info();
		assert_err!(
			extrinsic.apply::<Test>(&dispatch_info, 0),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				crate::TransactionValidationError::InvalidChainId as u8,
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
		let t = eip1559_erc20_creation_transaction(alice);

		assert_ok!(Ethereum::execute(alice.address, &t, None,));
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
			eip1559_erc20_creation_transaction(alice),
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
		let t = eip1559_erc20_creation_transaction(alice);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));
		assert_ne!(EVM::account_codes(erc20_address).len(), 0);
	});
}

#[test]
fn transaction_should_generate_correct_gas_used() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let expected_gas = U256::from(893928);

	ext.execute_with(|| {
		let t = eip1559_erc20_creation_transaction(alice);
		let (_, _, info) = Ethereum::execute(alice.address, &t, None).unwrap();

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
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = EIP1559UnsignedTransaction {
			nonce: U256::zero(),
			max_priority_fee_per_gas: U256::from(1),
			max_fee_per_gas: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Create,
			value: U256::zero(),
			input: hex::decode(CONTRACT).unwrap(),
		}
		.sign(&alice.private_key, None);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));

		let contract_address = hex::decode("32dcab0ef3fb2de2fce1d2e0799d36239671f04a").unwrap();
		let foo = hex::decode("c2985578").unwrap();
		let bar = hex::decode("febb0f7e").unwrap();

		let t2 = EIP1559UnsignedTransaction {
			nonce: U256::from(1),
			max_priority_fee_per_gas: U256::from(1),
			max_fee_per_gas: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: foo,
		}
		.sign(&alice.private_key, None);

		// calling foo will succeed
		let (_, _, info) = Ethereum::execute(alice.address, &t2, None).unwrap();

		match info {
			CallOrCreateInfo::Call(info) => {
				assert_eq!(
					hex::encode(info.value),
					"0000000000000000000000000000000000000000000000000000000000000001"
				);
			}
			CallOrCreateInfo::Create(_) => panic!("expected call info"),
		}

		let t3 = EIP1559UnsignedTransaction {
			nonce: U256::from(2),
			max_priority_fee_per_gas: U256::from(1),
			max_fee_per_gas: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: bar,
		}
		.sign(&alice.private_key, None);

		// calling should always succeed even if the inner EVM execution fails.
		Ethereum::execute(alice.address, &t3, None).ok().unwrap();
	});
}

#[test]
fn test_transact_xcm_evm_transfer() {
	let (pairs, mut ext) = new_test_ext(2);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let balances_before = System::account(&bob.account_id);
		Ethereum::transact_xcm(
			RawOrigin::XcmEthereumTransaction(alice.address).into(),
			xcm_evm_transfer_eip_1559_transaction(bob.address, U256::from(100)),
		)
		.expect("Failed to execute transaction");

		assert_eq!(
			System::account(&bob.account_id).data.free,
			balances_before.data.free + 100
		);
	});
}

#[test]
fn test_transact_xcm_create() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		assert_noop!(
			Ethereum::transact_xcm(
				RawOrigin::XcmEthereumTransaction(alice.address).into(),
				xcm_erc20_creation_eip_1559_transaction()
			),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(0),
					pays_fee: Pays::Yes,
				},
				error: DispatchError::Other("Cannot convert xcm payload to known type"),
			}
		);
	});
}

#[test]
fn test_transact_xcm_evm_call_works() {
	let (pairs, mut ext) = new_test_ext(2);
	let alice = &pairs[0];
	let bob = &pairs[1];

	ext.execute_with(|| {
		let t = EIP1559UnsignedTransaction {
			nonce: U256::zero(),
			max_priority_fee_per_gas: U256::from(1),
			max_fee_per_gas: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Create,
			value: U256::zero(),
			input: hex::decode(CONTRACT).unwrap(),
		}
		.sign(&alice.private_key, None);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));

		let contract_address = hex::decode("32dcab0ef3fb2de2fce1d2e0799d36239671f04a").unwrap();
		let foo = hex::decode("c2985578").unwrap();
		let bar = hex::decode("febb0f7e").unwrap();

		let _ = Ethereum::transact_xcm(
			RawOrigin::XcmEthereumTransaction(bob.address).into(),
			xcm_evm_call_eip_1559_transaction(H160::from_slice(&contract_address), foo),
		).expect("Failed to call `foo`");

		// Evm call failing still succesfully dispatched
		let _ = Ethereum::transact_xcm(
			RawOrigin::XcmEthereumTransaction(bob.address).into(),
			xcm_evm_call_eip_1559_transaction(H160::from_slice(&contract_address), bar),
		).expect("Failed to call `bar`");

		let pending = crate::Pending::<Test>::get();
		assert!(pending.len() == 2);

		// Transaction is in Pending storage, with nonce 0 and status 1 (evm succeed).
		let (transaction_0, _, receipt_0) = &pending[0];
		match (transaction_0, receipt_0) {
			(&crate::Transaction::EIP1559(ref t), &crate::Receipt::EIP1559(ref r)) => {
				assert!(t.nonce == U256::from(0u8));
				assert!(r.status_code == 1u8);
			},
			_ => unreachable!(),
		}

		// Transaction is in Pending storage, with nonce 1 and status 0 (evm failed).
		let (transaction_1, _, receipt_1) = &pending[1];
		match (transaction_1, receipt_1) {
			(&crate::Transaction::EIP1559(ref t), &crate::Receipt::EIP1559(ref r)) => {
				assert!(t.nonce == U256::from(1u8));
				assert!(r.status_code == 0u8);
			},
			_ => unreachable!(),
		}
	});
}
