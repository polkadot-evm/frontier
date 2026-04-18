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

use super::*;
use crate::mock::*;

use evm::ExitReason;
use frame_support::{
	assert_ok,
	traits::{LockIdentifier, LockableCurrency, WithdrawReasons},
};
use sp_runtime::BuildStorage;
use std::{collections::BTreeMap, str::FromStr};

mod proof_size_test {
	use super::*;
	use fp_evm::{
		CreateInfo, ACCOUNT_BASIC_PROOF_SIZE, ACCOUNT_CODES_METADATA_PROOF_SIZE,
		ACCOUNT_STORAGE_PROOF_SIZE, IS_EMPTY_CHECK_PROOF_SIZE, WRITE_PROOF_SIZE,
	};
	use frame_support::traits::StorageInfoTrait;
	// pragma solidity ^0.8.2;
	// contract Callee {
	//     // ac4c25b2
	//     function void() public {
	//         uint256 foo = 1;
	//     }
	// }
	pub const PROOF_SIZE_TEST_CALLEE_CONTRACT_BYTECODE: &str =
		include_str!("./res/proof_size_test_callee_contract_bytecode.txt");
	// pragma solidity ^0.8.2;
	// contract ProofSizeTest {
	//     uint256 foo;
	//     constructor() {
	//         foo = 6;
	//     }
	//     // 35f56c3b
	//     function test_balance(address who) public {
	//         // cold
	//         uint256 a = address(who).balance;
	//         // warm
	//         uint256 b = address(who).balance;
	//     }
	//     // e27a0ecd
	//     function test_sload() public returns (uint256) {
	//         // cold
	//         uint256 a = foo;
	//         // warm
	//         uint256 b = foo;
	//         return b;
	//     }
	//     // 4f3080a9
	//     function test_sstore() public {
	//         // cold
	//         foo = 4;
	//         // warm
	//         foo = 5;
	//     }
	//     // c6d6f606
	//     function test_call(Callee _callee) public {
	//         _callee.void();
	//     }
	//     // 944ddc62
	//     function test_oog() public {
	//         uint256 i = 1;
	//         while(true) {
	//             address who = address(uint160(uint256(keccak256(abi.encodePacked(bytes32(i))))));
	//             uint256 a = address(who).balance;
	//             i = i + 1;
	//         }
	//     }
	// }
	pub const PROOF_SIZE_TEST_CONTRACT_BYTECODE: &str =
		include_str!("./res/proof_size_test_contract_bytecode.txt");

	fn create_proof_size_test_callee_contract(
		gas_limit: u64,
		weight_limit: Option<Weight>,
	) -> Result<CreateInfo, crate::RunnerError<crate::Error<Test>>> {
		<Test as Config>::Runner::create(
			H160::default(),
			hex::decode(PROOF_SIZE_TEST_CALLEE_CONTRACT_BYTECODE.trim_end()).unwrap(),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			weight_limit,
			Some(0),
			&<Test as Config>::config().clone(),
		)
	}

	fn create_proof_size_test_contract(
		gas_limit: u64,
		weight_limit: Option<Weight>,
	) -> Result<CreateInfo, crate::RunnerError<crate::Error<Test>>> {
		<Test as Config>::Runner::create(
			H160::default(),
			hex::decode(PROOF_SIZE_TEST_CONTRACT_BYTECODE.trim_end()).unwrap(),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // non-transactional
			true, // must be validated
			weight_limit,
			Some(0),
			&<Test as Config>::config().clone(),
		)
	}

	#[test]
	fn account_basic_proof_size_constant_matches() {
		assert_eq!(
			ACCOUNT_BASIC_PROOF_SIZE,
			frame_system::Account::<Test>::storage_info()
				.first()
				.expect("item")
				.max_size
				.expect("size") as u64
		);
	}

	#[test]
	fn account_storage_proof_size_constant_matches() {
		assert_eq!(
			ACCOUNT_STORAGE_PROOF_SIZE,
			AccountStorages::<Test>::storage_info()
				.first()
				.expect("item")
				.max_size
				.expect("size") as u64
		);
	}

	#[test]
	fn account_codes_metadata_proof_size_constant_matches() {
		assert_eq!(
			ACCOUNT_CODES_METADATA_PROOF_SIZE,
			AccountCodesMetadata::<Test>::storage_info()
				.first()
				.expect("item")
				.max_size
				.expect("size") as u64
		);
	}

	#[test]
	fn proof_size_create_accounting_works() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 1_000_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

			let result = create_proof_size_test_callee_contract(gas_limit, Some(weight_limit))
				.expect("create succeeds");

			// Creating a new contract does not involve reading the code from storage.
			// We account for a fixed hash proof size write, an empty check and .
			let write_cost = WRITE_PROOF_SIZE;
			let is_empty_check = IS_EMPTY_CHECK_PROOF_SIZE;
			let nonce_increases = ACCOUNT_BASIC_PROOF_SIZE * 2;
			let expected_proof_size = write_cost + is_empty_check + nonce_increases;

			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(expected_proof_size, actual_proof_size);
		});
	}

	#[test]
	fn proof_size_subcall_accounting_works() {
		new_test_ext().execute_with(|| {
			// Create callee contract A
			let gas_limit: u64 = 1_000_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);
			let result =
				create_proof_size_test_callee_contract(gas_limit, None).expect("create succeeds");

			let subcall_contract_address = result.value;

			// Create proof size test contract B
			let result = create_proof_size_test_contract(gas_limit, None).expect("create succeeds");

			let call_contract_address = result.value;

			// Call B, that calls A, with weight limit
			// selector for ProofSizeTest::test_call function..
			let mut call_data: String = "c6d6f606000000000000000000000000".to_owned();
			// ..encode the callee address argument
			call_data.push_str(&format!("{subcall_contract_address:x}"));

			let result = <Test as Config>::Runner::call(
				H160::default(),
				call_contract_address,
				hex::decode(&call_data).unwrap(),
				U256::zero(),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&<Test as Config>::config().clone(),
			)
			.expect("call succeeds");

			// Expected proof size
			let reading_main_contract_len = AccountCodes::<Test>::get(call_contract_address).len();
			let reading_contract_len = AccountCodes::<Test>::get(subcall_contract_address).len();
			let read_account_metadata = ACCOUNT_CODES_METADATA_PROOF_SIZE as usize;
			let is_empty_check = (IS_EMPTY_CHECK_PROOF_SIZE * 2) as usize;
			let increase_nonce = (ACCOUNT_BASIC_PROOF_SIZE * 3) as usize;
			let expected_proof_size = ((read_account_metadata * 2)
				+ reading_contract_len
				+ reading_main_contract_len
				+ is_empty_check
				+ increase_nonce) as u64;

			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(expected_proof_size, actual_proof_size);
		});
	}

	#[test]
	fn proof_size_balance_accounting_works() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 1_000_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

			// Create proof size test contract
			let result = create_proof_size_test_contract(gas_limit, None).expect("create succeeds");

			let call_contract_address = result.value;

			// selector for ProofSizeTest::balance function..
			let mut call_data: String = "35f56c3b000000000000000000000000".to_owned();
			// ..encode bobs address
			call_data.push_str(&format!("{:x}", H160::random()));

			let result = <Test as Config>::Runner::call(
				H160::default(),
				call_contract_address,
				hex::decode(&call_data).unwrap(),
				U256::zero(),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&<Test as Config>::config().clone(),
			)
			.expect("call succeeds");

			// - Three account reads.
			// - Main contract code read.
			// - One metadata read.
			let basic_account_size = (ACCOUNT_BASIC_PROOF_SIZE * 3) as usize;
			let read_account_metadata = ACCOUNT_CODES_METADATA_PROOF_SIZE as usize;
			let is_empty_check = IS_EMPTY_CHECK_PROOF_SIZE as usize;
			let increase_nonce = ACCOUNT_BASIC_PROOF_SIZE as usize;
			let reading_main_contract_len = AccountCodes::<Test>::get(call_contract_address).len();
			let expected_proof_size = (basic_account_size
				+ read_account_metadata
				+ reading_main_contract_len
				+ is_empty_check
				+ increase_nonce) as u64;

			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(expected_proof_size, actual_proof_size);
		});
	}

	#[test]
	fn proof_size_sload_accounting_works() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 1_000_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

			// Create proof size test contract
			let result = create_proof_size_test_contract(gas_limit, None).expect("create succeeds");

			let call_contract_address = result.value;

			// selector for ProofSizeTest::test_sload function..
			let call_data: String = "e27a0ecd".to_owned();
			let result = <Test as Config>::Runner::call(
				H160::default(),
				call_contract_address,
				hex::decode(call_data).unwrap(),
				U256::zero(),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&<Test as Config>::config().clone(),
			)
			.expect("call succeeds");

			let reading_main_contract_len =
				AccountCodes::<Test>::get(call_contract_address).len() as u64;
			let expected_proof_size = reading_main_contract_len
				+ ACCOUNT_STORAGE_PROOF_SIZE
				+ ACCOUNT_CODES_METADATA_PROOF_SIZE
				+ IS_EMPTY_CHECK_PROOF_SIZE
				+ (ACCOUNT_BASIC_PROOF_SIZE * 2);

			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(expected_proof_size, actual_proof_size);
		});
	}

	#[test]
	fn proof_size_sstore_accounting_works() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 1_000_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

			// Create proof size test contract
			let result = create_proof_size_test_contract(gas_limit, None).expect("create succeeds");

			let call_contract_address = result.value;

			// selector for ProofSizeTest::test_sstore function..
			let call_data: String = "4f3080a9".to_owned();
			let result = <Test as Config>::Runner::call(
				H160::default(),
				call_contract_address,
				hex::decode(call_data).unwrap(),
				U256::zero(),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&<Test as Config>::config().clone(),
			)
			.expect("call succeeds");

			let reading_main_contract_len =
				AccountCodes::<Test>::get(call_contract_address).len() as u64;
			let expected_proof_size = reading_main_contract_len
				+ WRITE_PROOF_SIZE
				+ ACCOUNT_CODES_METADATA_PROOF_SIZE
				+ ACCOUNT_STORAGE_PROOF_SIZE
				+ IS_EMPTY_CHECK_PROOF_SIZE
				+ (ACCOUNT_BASIC_PROOF_SIZE * 2);

			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(expected_proof_size, actual_proof_size);
		});
	}

	#[test]
	fn proof_size_oog_works() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 1_000_000;
			let mut weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

			// Artifically set a lower proof size limit so we OOG this instead gas.
			let proof_size_limit = weight_limit.proof_size() / 2;
			*weight_limit.proof_size_mut() = proof_size_limit;

			// Create proof size test contract
			let result = create_proof_size_test_contract(gas_limit, None).expect("create succeeds");

			let call_contract_address = result.value;

			// selector for ProofSizeTest::test_oog function..
			let call_data: String = "944ddc62".to_owned();
			let result = <Test as Config>::Runner::call(
				H160::default(),
				call_contract_address,
				hex::decode(call_data).unwrap(),
				U256::zero(),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&<Test as Config>::config().clone(),
			)
			.expect("call succeeds");

			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(proof_size_limit, actual_proof_size);
		});
	}

	#[test]
	fn uncached_account_code_proof_size_accounting_works() {
		new_test_ext().execute_with(|| {
			// Create callee contract A
			let gas_limit: u64 = 1_000_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);
			let result =
				create_proof_size_test_callee_contract(gas_limit, None).expect("create succeeds");

			let subcall_contract_address = result.value;

			// Expect callee contract code hash and size to be cached
			let _ = <AccountCodesMetadata<Test>>::get(subcall_contract_address)
				.expect("contract code hash and size are cached");

			// Remove callee cache
			<AccountCodesMetadata<Test>>::remove(subcall_contract_address);

			// Create proof size test contract B
			let result = create_proof_size_test_contract(gas_limit, None).expect("create succeeds");

			let call_contract_address = result.value;

			// Call B, that calls A, with weight limit
			// selector for ProofSizeTest::test_call function..
			let mut call_data: String = "c6d6f606000000000000000000000000".to_owned();
			// ..encode the callee address argument
			call_data.push_str(&format!("{subcall_contract_address:x}"));
			let result = <Test as Config>::Runner::call(
				H160::default(),
				call_contract_address,
				hex::decode(&call_data).unwrap(),
				U256::zero(),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&<Test as Config>::config().clone(),
			)
			.expect("call succeeds");

			// Expected proof size
			let read_account_metadata = ACCOUNT_CODES_METADATA_PROOF_SIZE as usize;
			let is_empty_check = (IS_EMPTY_CHECK_PROOF_SIZE * 2) as usize;
			let increase_nonce = (ACCOUNT_BASIC_PROOF_SIZE * 3) as usize;
			let reading_main_contract_len = AccountCodes::<Test>::get(call_contract_address).len();
			let reading_callee_contract_len =
				AccountCodes::<Test>::get(subcall_contract_address).len();
			// In order to do the subcall, we need to check metadata 3 times -
			// one for each contract + one for the call opcode -, load two bytecodes - caller and callee.
			let expected_proof_size = ((read_account_metadata * 2)
				+ reading_callee_contract_len
				+ reading_main_contract_len
				+ is_empty_check
				+ increase_nonce) as u64;

			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(expected_proof_size, actual_proof_size);
		});
	}

	#[test]
	fn proof_size_breaks_standard_transfer() {
		new_test_ext().execute_with(|| {
			// In this test we do a simple transfer to an address with an stored code which is
			// greater in size (and thus load cost) than the transfer flat fee of 21_000.

			// We assert that providing 21_000 gas limit will not work, because the pov size limit
			// will OOG.
			let fake_contract_address = H160::random();
			let config = <Test as Config>::config().clone();
			let fake_contract_code = vec![0; config.create_contract_limit.expect("a value")];
			AccountCodes::<Test>::insert(fake_contract_address, fake_contract_code);

			let gas_limit: u64 = 21_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

			let result = <Test as Config>::Runner::call(
				H160::default(),
				fake_contract_address,
				Vec::new(),
				U256::from(777),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&config,
			)
			.expect("call succeeds");

			assert_eq!(
				result.exit_reason,
				crate::ExitReason::Error(crate::ExitError::OutOfGas)
			);
		});
	}

	#[test]
	fn proof_size_based_refunding_works() {
		new_test_ext().execute_with(|| {
			// In this test we do a simple transfer to an address with an stored code which is
			// greater in size (and thus load cost) than the transfer flat fee of 21_000.

			// Assert that if we provide enough gas limit, the refund will be based on the pov
			// size consumption, not the 21_000 gas.
			let fake_contract_address = H160::random();
			let config = <Test as Config>::config().clone();
			let fake_contract_code = vec![0; config.create_contract_limit.expect("a value")];
			AccountCodes::<Test>::insert(fake_contract_address, fake_contract_code);

			let gas_limit: u64 = 700_000;
			let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

			let result = <Test as Config>::Runner::call(
				H160::default(),
				fake_contract_address,
				Vec::new(),
				U256::from(777),
				gas_limit,
				Some(FixedGasPrice::min_gas_price().0),
				None,
				None,
				Vec::new(),
				Vec::new(),
				true, // transactional
				true, // must be validated
				Some(weight_limit),
				Some(0),
				None,
				&config,
			)
			.expect("call succeeds");

			let ratio = <<Test as Config>::GasLimitPovSizeRatio as Get<u64>>::get();
			let used_gas = result.used_gas;
			let actual_proof_size = result
				.weight_info
				.expect("weight info")
				.proof_size_usage
				.expect("proof size usage");

			assert_eq!(used_gas.standard, U256::from(21_000));
			assert_eq!(used_gas.effective, U256::from(actual_proof_size * ratio));
		});
	}
}

mod storage_growth_test {
	use super::*;
	use crate::tests::proof_size_test::PROOF_SIZE_TEST_CALLEE_CONTRACT_BYTECODE;
	use fp_evm::{
		ACCOUNT_CODES_KEY_SIZE, ACCOUNT_CODES_METADATA_PROOF_SIZE, ACCOUNT_STORAGE_PROOF_SIZE,
	};

	const PROOF_SIZE_CALLEE_CONTRACT_BYTECODE_LEN: u64 = 116;
	// The contract bytecode stored on chain.
	const STORAGE_GROWTH_TEST_CONTRACT: &str =
		include_str!("./res/storage_growth_test_contract_bytecode.txt");
	const STORAGE_GROWTH_TEST_CONTRACT_BYTECODE_LEN: u64 = 455;

	fn create_test_contract(
		contract: &str,
		gas_limit: u64,
	) -> Result<CreateInfo, crate::RunnerError<crate::Error<Test>>> {
		<Test as Config>::Runner::create(
			H160::default(),
			hex::decode(contract.trim_end()).expect("Failed to decode contract"),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(FixedGasWeightMapping::<Test>::gas_to_weight(
				gas_limit, true,
			)),
			Some(0),
			<Test as Config>::config(),
		)
	}

	// Calls the given contract
	fn call_test_contract(
		contract_addr: H160,
		call_data: &[u8],
		value: U256,
		gas_limit: u64,
	) -> Result<CallInfo, crate::RunnerError<crate::Error<Test>>> {
		<Test as Config>::Runner::call(
			H160::default(),
			contract_addr,
			call_data.to_vec(),
			value,
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			None,
			Some(0),
			None,
			<Test as Config>::config(),
		)
	}

	// Computes the expected gas for contract creation (related to storage growth).
	// `byte_code_len` represents the length of the contract bytecode stored on-chain.
	fn expected_contract_create_storage_growth_gas(bytecode_len: u64) -> u64 {
		let ratio = <<Test as Config>::GasLimitStorageGrowthRatio as Get<u64>>::get();
		(ACCOUNT_CODES_KEY_SIZE + ACCOUNT_CODES_METADATA_PROOF_SIZE + bytecode_len) * ratio
	}

	/// Test that contract deployment succeeds when the necessary storage growth gas is provided.
	#[test]
	fn contract_deployment_should_succeed() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 85_000;

			let result = create_test_contract(PROOF_SIZE_TEST_CALLEE_CONTRACT_BYTECODE, gas_limit)
				.expect("create succeeds");

			assert_eq!(
				result.used_gas.effective.as_u64(),
				expected_contract_create_storage_growth_gas(
					PROOF_SIZE_CALLEE_CONTRACT_BYTECODE_LEN
				)
			);
			assert_eq!(
				result.exit_reason,
				crate::ExitReason::Succeed(ExitSucceed::Returned)
			);
			// Assert that the contract entry exists in the storage.
			assert!(AccountCodes::<Test>::contains_key(result.value));
		});
	}

	// Test that contract creation with code initialization that results in new storage entries
	// succeeds when the necessary storage growth gas is provided.
	#[test]
	fn contract_creation_with_code_initialization_should_succeed() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 863_394;
			let ratio = <<Test as Config>::GasLimitStorageGrowthRatio as Get<u64>>::get();
			// The constructor of the contract creates 3 new storage entries (uint256). So,
			// the expected gas is the gas for contract creation + 3 * ACCOUNT_STORAGE_PROOF_SIZE.
			let expected_storage_growth_gas = expected_contract_create_storage_growth_gas(
				STORAGE_GROWTH_TEST_CONTRACT_BYTECODE_LEN,
			) + (3 * ACCOUNT_STORAGE_PROOF_SIZE * ratio);

			// Deploy the contract.
			let result = create_test_contract(STORAGE_GROWTH_TEST_CONTRACT, gas_limit)
				.expect("create succeeds");

			assert_eq!(
				result.used_gas.effective.as_u64(),
				expected_storage_growth_gas
			);
			assert_eq!(
				result.exit_reason,
				crate::ExitReason::Succeed(ExitSucceed::Returned)
			);
		});
	}

	// Verify that saving new entries fails when insufficient storage growth gas is supplied.
	#[test]
	fn store_new_entries_should_fail_oog() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 863_394;
			// Deploy the contract.
			let res = create_test_contract(STORAGE_GROWTH_TEST_CONTRACT, gas_limit)
				.expect("create succeeds");
			let contract_addr = res.value;

			let gas_limit = 120_000;
			// Call the contract method store to store new entries.
			let result = call_test_contract(
				contract_addr,
				&hex::decode("975057e7").unwrap(),
				U256::zero(),
				gas_limit,
			)
			.expect("call should succeed");

			assert_eq!(
				result.exit_reason,
				crate::ExitReason::Error(crate::ExitError::OutOfGas)
			);
		});
	}

	// Verify that saving new entries succeeds when sufficient storage growth gas is supplied.
	#[test]
	fn store_new_entries_should_succeeds() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 863_394;
			// Deploy the contract.
			let res = create_test_contract(STORAGE_GROWTH_TEST_CONTRACT, gas_limit)
				.expect("create succeeds");
			let contract_addr = res.value;

			let gas_limit = 128_000;
			// Call the contract method store to store new entries.
			let result = call_test_contract(
				contract_addr,
				&hex::decode("975057e7").unwrap(),
				U256::zero(),
				gas_limit,
			)
			.expect("call should succeed");

			let expected_storage_growth_gas = 3
				* ACCOUNT_STORAGE_PROOF_SIZE
				* <<Test as Config>::GasLimitStorageGrowthRatio as Get<u64>>::get();
			assert_eq!(
				result.exit_reason,
				crate::ExitReason::Succeed(ExitSucceed::Stopped)
			);
			assert_eq!(
				result.used_gas.effective.as_u64(),
				expected_storage_growth_gas
			);
		});
	}

	// Verify that updating existing storage entries does not incur any storage growth charges.
	#[test]
	fn update_exisiting_entries_succeeds() {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 863_394;
			// Deploy the contract.
			let res = create_test_contract(STORAGE_GROWTH_TEST_CONTRACT, gas_limit)
				.expect("create succeeds");
			let contract_addr = res.value;

			// Providing gas limit of 37_000 is enough to update existing entries, but not enough
			// to store new entries.
			let gas_limit = 37_000;
			// Call the contract method update to update existing entries.
			let result = call_test_contract(
				contract_addr,
				&hex::decode("a2e62045").unwrap(),
				U256::zero(),
				gas_limit,
			)
			.expect("call should succeed");

			assert_eq!(
				result.exit_reason,
				crate::ExitReason::Succeed(ExitSucceed::Stopped)
			);
		});
	}
}

type Balances = pallet_balances::Pallet<Test>;
#[allow(clippy::upper_case_acronyms)]
type EVM = Pallet<Test>;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default()
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
	accounts.insert(
		H160::from([4u8; 20]), // alith
		GenesisAccount {
			nonce: U256::from(1),
			balance: U256::max_value(),
			storage: Default::default(),
			code: vec![],
		},
	);
	accounts.insert(
		H160::from([5u8; 20]), // bob
		GenesisAccount {
			nonce: U256::from(1),
			balance: U256::max_value(),
			storage: Default::default(),
			code: vec![],
		},
	);
	accounts.insert(
		H160::from([6u8; 20]), // charleth
		GenesisAccount {
			nonce: U256::from(1),
			balance: U256::max_value(),
			storage: Default::default(),
			code: vec![],
		},
	);

	// Create the block author account with some balance.
	let author = H160::from_str("0x1234500000000000000000000000000000000000").unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(
			<Test as Config>::AddressMapping::into_account_id(author),
			12345,
		)],
		dev_accounts: None,
	}
	.assimilate_storage(&mut t)
	.expect("Pallet balances storage can be assimilated");

	crate::GenesisConfig::<Test> {
		accounts,
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}

// pragma solidity ^0.8.2;

// contract Foo {

//  function newBar() // 2fc11060
//    public
//    returns(Bar newContract)
//  {
//    Bar b = new Bar();
//    return b;
//  }
//}

// contract Bar {
//  function getNumber()
//    public
//    pure
//    returns (uint32 number)
//  {
//    return 10;
//  }
//}
pub const FOO_BAR_CONTRACT_CREATOR_BYTECODE: &str =
	include_str!("./res/foo_bar_contract_creator.txt");

fn create_foo_bar_contract_creator(
	gas_limit: u64,
	weight_limit: Option<Weight>,
) -> Result<CreateInfo, crate::RunnerError<crate::Error<Test>>> {
	<Test as Config>::Runner::create(
		H160::default(),
		hex::decode(FOO_BAR_CONTRACT_CREATOR_BYTECODE.trim_end()).unwrap(),
		U256::zero(),
		gas_limit,
		Some(FixedGasPrice::min_gas_price().0),
		None,
		None,
		Vec::new(),
		Vec::new(),
		true, // transactional
		true, // must be validated
		weight_limit,
		Some(0),
		&<Test as Config>::config().clone(),
	)
}

#[test]
fn test_contract_deploy_succeeds_if_address_is_allowed() {
	new_test_ext().execute_with(|| {
		let gas_limit: u64 = 1_000_000;
		let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

		assert!(<Test as Config>::Runner::create(
			// Alith is allowed to deploy contracts
			H160::from([4u8; 20]),
			hex::decode(FOO_BAR_CONTRACT_CREATOR_BYTECODE.trim_end()).unwrap(),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(weight_limit),
			Some(0),
			&<Test as Config>::config().clone(),
		)
		.is_ok());
	});
}

#[test]
fn test_contract_deploy_fails_if_address_not_allowed() {
	new_test_ext().execute_with(|| {
		let gas_limit: u64 = 1_000_000;
		let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

		match <Test as Config>::Runner::create(
			// Bob is not allowed to deploy contracts
			H160::from([5u8; 20]),
			hex::decode(FOO_BAR_CONTRACT_CREATOR_BYTECODE.trim_end()).unwrap(),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(weight_limit),
			Some(0),
			&<Test as Config>::config().clone(),
		) {
			Err(RunnerError {
				error: Error::CreateOriginNotAllowed,
				..
			}) => (),
			_ => panic!("Should have failed with CreateOriginNotAllowed"),
		}
	});
}

#[test]
fn test_inner_contract_deploy_succeeds_if_address_is_allowed() {
	new_test_ext().execute_with(|| {
		let gas_limit: u64 = 1_000_000;
		let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

		let result1 = create_foo_bar_contract_creator(gas_limit, Some(weight_limit))
			.expect("create succeeds");

		let call_data: String = "2fc11060".to_owned();
		let call_contract_address = result1.value;

		let result = <Test as Config>::Runner::call(
			// Alith is allowed to deploy inner contracts
			H160::from([4u8; 20]),
			call_contract_address,
			hex::decode(&call_data).unwrap(),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(weight_limit),
			Some(0),
			None,
			&<Test as Config>::config().clone(),
		)
		.expect("call succeeds");

		assert_eq!(
			result.exit_reason,
			ExitReason::Succeed(ExitSucceed::Returned)
		);
	});
}

#[test]
fn test_inner_contract_deploy_reverts_if_address_not_allowed() {
	new_test_ext().execute_with(|| {
		let gas_limit: u64 = 1_000_000;
		let weight_limit = FixedGasWeightMapping::<Test>::gas_to_weight(gas_limit, true);

		let result1 = create_foo_bar_contract_creator(gas_limit, Some(weight_limit))
			.expect("create succeeds");

		let call_data: String = "2fc11060".to_owned();
		let call_contract_address = result1.value;

		let result = <Test as Config>::Runner::call(
			// Charleth is not allowed to deploy inner contracts
			H160::from([6u8; 20]),
			call_contract_address,
			hex::decode(&call_data).unwrap(),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			Some(weight_limit),
			Some(0),
			None,
			&<Test as Config>::config().clone(),
		)
		.expect("call succeeds");

		assert_eq!(result.exit_reason, ExitReason::Revert(ExitRevert::Reverted));
	});
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
		assert_eq!(Balances::free_balance(&substrate_addr), 100);

		// Deduct fees as 10 units
		let imbalance = <<Test as Config>::OnChargeTransaction as OnChargeEVMTransaction<Test>>::withdraw_fee(&evm_addr, U256::from(10)).unwrap();
		assert_eq!(Balances::free_balance(&substrate_addr), 90);

		// Refund fees as 5 units
		<<Test as Config>::OnChargeTransaction as OnChargeEVMTransaction<Test>>::correct_and_deposit_fee(&evm_addr, U256::from(5), U256::from(5), imbalance);
		assert_eq!(Balances::free_balance(&substrate_addr), 95);
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
		assert_eq!(Balances::free_balance(&substrate_addr), 21_777_000_000_000);

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
			Vec::new(),
		);
		// All that was due, was refunded.
		assert_eq!(Balances::free_balance(&substrate_addr), 776_000_000_000);
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
		assert_eq!(Balances::free_balance(&substrate_addr), 100);

		// Drain funds
		let _ =
			<<Test as Config>::OnChargeTransaction as OnChargeEVMTransaction<Test>>::withdraw_fee(
				&evm_addr,
				U256::from(100),
			)
			.unwrap();
		assert_eq!(Balances::free_balance(&substrate_addr), 0);

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
		assert!(EVM::create_account(addr_2, vec![1, 2, 3], None).is_ok());
		let account_2 = frame_system::Account::<Test>::get(substrate_addr_2.clone());
		// We increased the sufficient reference by 1.
		assert_eq!(account_2.sufficients, 1);
		EVM::remove_account(&addr_2);
		let account_2 = frame_system::Account::<Test>::get(substrate_addr_2);
		assert_eq!(account_2.sufficients, 0);
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
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			None,
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
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			None,
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
			Vec::new(),
			true, // transactional
			true, // must be validated
			None,
			None,
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
			Vec::new(),
			true, // transactional
			true, // must be validated
			None,
			None,
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
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			None,
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
			Vec::new(),
			true,  // transactional
			false, // not sure be validated
			None,
			None,
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
			Vec::new(),
			false, // non-transactional
			true,  // must be validated
			None,
			None,
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

		assert!(crate::Pallet::<Test>::create_account(address, b"Exemple".to_vec(), None).is_ok());

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

/// EIP-7939: CLZ (Count Leading Zeros) opcode tests
///
/// The CLZ opcode (0x1e) counts the number of leading zero bits in a 256-bit value.
/// Test vectors from https://eips.ethereum.org/EIPS/eip-7939
mod eip7939_clz_test {
	use super::*;
	use evm::ExitSucceed;
	use fp_evm::CreateInfo;

	// All contracts use: PUSH<n> value, CLZ, PUSH1 0, MSTORE, PUSH1 32, PUSH1 0, RETURN
	// For PUSH1 values (11 bytes runtime): PUSH11 pattern with MSTORE
	// For PUSH32 values (42 bytes runtime): CODECOPY pattern

	// CLZ(0x00) = 256
	const CLZ_ZERO: &str = "6a60001e60005260206000f3600052600b6015f3";

	// CLZ(0x01) = 255
	const CLZ_ONE: &str = "6a60011e60005260206000f3600052600b6015f3";

	// CLZ(0x8000...00) = 0 (2^255, MSB set)
	const CLZ_MSB_SET: &str = "602a600c600039602a6000f37f80000000000000000000000000000000000000000000000000000000000000001e60005260206000f3";

	// CLZ(0xffff...ff) = 0 (MAX_U256, all bits set)
	const CLZ_MAX_U256: &str = "602a600c600039602a6000f37fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1e60005260206000f3";

	// CLZ(0x4000...00) = 1 (2^254)
	const CLZ_SECOND_BIT: &str = "602a600c600039602a6000f37f40000000000000000000000000000000000000000000000000000000000000001e60005260206000f3";

	// CLZ(0x7fff...ff) = 1 (2^255 - 1)
	const CLZ_MSB_UNSET: &str = "602a600c600039602a6000f37f7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1e60005260206000f3";

	fn create_clz_test_contract(
		contract: &str,
		gas_limit: u64,
	) -> Result<CreateInfo, crate::RunnerError<crate::Error<Test>>> {
		<Test as Config>::Runner::create(
			H160::default(),
			hex::decode(contract.trim_end()).expect("Failed to decode contract"),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			None,
			Some(0),
			<Test as Config>::config(),
		)
	}

	fn call_contract(
		contract_addr: H160,
		gas_limit: u64,
	) -> Result<CallInfo, crate::RunnerError<crate::Error<Test>>> {
		<Test as Config>::Runner::call(
			H160::default(),
			contract_addr,
			Vec::new(),
			U256::zero(),
			gas_limit,
			Some(FixedGasPrice::min_gas_price().0),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true, // transactional
			true, // must be validated
			None,
			Some(0),
			None,
			<Test as Config>::config(),
		)
	}

	fn u256_to_bytes(value: u16) -> Vec<u8> {
		let mut result = vec![0u8; 32];
		result[30] = (value >> 8) as u8;
		result[31] = value as u8;
		result
	}

	fn run_clz_test(contract: &str, expected_clz: u16) {
		new_test_ext().execute_with(|| {
			let gas_limit: u64 = 1_000_000;

			let result = create_clz_test_contract(contract, gas_limit)
				.expect("contract deployment should succeed");
			assert_eq!(
				result.exit_reason,
				crate::ExitReason::Succeed(ExitSucceed::Returned)
			);

			let call_result =
				call_contract(result.value, gas_limit).expect("contract call should succeed");
			assert_eq!(
				call_result.exit_reason,
				crate::ExitReason::Succeed(ExitSucceed::Returned)
			);

			assert_eq!(call_result.value, u256_to_bytes(expected_clz));
		});
	}

	// EIP-7939 Test Vectors

	#[test]
	fn clz_of_0x00_returns_256() {
		run_clz_test(CLZ_ZERO, 256);
	}

	#[test]
	fn clz_of_0x01_returns_255() {
		run_clz_test(CLZ_ONE, 255);
	}

	#[test]
	fn clz_of_0x8000_returns_0() {
		run_clz_test(CLZ_MSB_SET, 0);
	}

	#[test]
	fn clz_of_0xffff_returns_0() {
		run_clz_test(CLZ_MAX_U256, 0);
	}

	#[test]
	fn clz_of_0x4000_returns_1() {
		run_clz_test(CLZ_SECOND_BIT, 1);
	}

	#[test]
	fn clz_of_0x7fff_returns_1() {
		run_clz_test(CLZ_MSB_UNSET, 1);
	}
}

/// Tests for the `eth_call`-style state override feature.
///
/// These tests exercise `Runner::call`'s `state_override` parameter, which is the
/// efficient Geth-style full-storage replacement introduced for the RPC layer
/// (runtime API v7). The semantics we verify here:
///
/// * A non-`None` override entry for an account causes storage reads for that
///   account to be served **exclusively** from the in-memory override map,
///   mirroring Geth's "destructed and recreated" state object. Slots not present
///   in the override map read as zero, regardless of on-chain state.
/// * An empty override map for an account (`"state": {}`) performs a full wipe.
/// * Accounts not listed in the override map are unaffected and read from on-chain
///   storage normally.
/// * Writes within the overridden call (SSTORE) take precedence over the override
///   baseline for subsequent reads in that call.
/// * The override is ephemeral: on-chain `AccountStorages` is not mutated.
/// * `reset_storage` (invoked e.g. by SELFDESTRUCT) clears any active override
///   entry for the affected address.
mod state_override_test {
	use super::*;
	use crate::runner::stack::SubstrateStackState;
	use evm::ExitSucceed;

	// Minimal runtime bytecode that returns `storage[calldata[0..32]]` as a 32-byte value.
	//
	//   PUSH1 0x00   // 60 00    (calldata offset)
	//   CALLDATALOAD // 35       (slot = calldata[0..32])
	//   SLOAD        // 54       (value = storage[slot])
	//   PUSH1 0x00   // 60 00    (memory offset)
	//   MSTORE       // 52       (mem[0..32] = value)
	//   PUSH1 0x20   // 60 20    (length = 32)
	//   PUSH1 0x00   // 60 00    (memory offset)
	//   RETURN       // f3
	const SLOAD_RETURN_BYTECODE: &str = "6000355460005260206000f3";

	// Runtime bytecode that SSTOREs `calldata[32..64]` at `calldata[0..32]`, then
	// SLOADs that slot and returns the value. Used to verify that writes during
	// the call take precedence over the state-override baseline.
	//
	//   PUSH1 0x20, CALLDATALOAD  // value
	//   PUSH1 0x00, CALLDATALOAD  // slot
	//   SSTORE                    // storage[slot] = value
	//   PUSH1 0x00, CALLDATALOAD  // slot
	//   SLOAD                     // value (from executor cache after SSTORE)
	//   PUSH1 0x00, MSTORE
	//   PUSH1 0x20, PUSH1 0x00, RETURN
	const SSTORE_THEN_SLOAD_BYTECODE: &str = "602035600035556000355460005260206000f3";

	fn slot(v: u64) -> H256 {
		H256::from_low_u64_be(v)
	}

	fn value(v: u64) -> H256 {
		H256::from_low_u64_be(v)
	}

	fn slot_calldata(slot_idx: u64) -> Vec<u8> {
		slot(slot_idx).as_bytes().to_vec()
	}

	fn slot_value_calldata(slot_idx: u64, val: u64) -> Vec<u8> {
		let mut data = Vec::with_capacity(64);
		data.extend_from_slice(slot(slot_idx).as_bytes());
		data.extend_from_slice(value(val).as_bytes());
		data
	}

	fn expected_return(val: u64) -> Vec<u8> {
		value(val).as_bytes().to_vec()
	}

	// Deploy a runtime bytecode directly at `address`, bypassing the constructor.
	fn deploy_runtime_code(address: H160, bytecode_hex: &str) {
		let bytecode = hex::decode(bytecode_hex).expect("valid hex bytecode");
		crate::Pallet::<Test>::create_account(address, bytecode, None)
			.expect("account creation succeeds");
	}

	// Execute an eth_call-style simulation (non-transactional, no fee) against `to`
	// with the given override payload.
	fn eth_call_with_override(
		to: H160,
		calldata: Vec<u8>,
		state_override: fp_evm::StateOverride,
	) -> CallInfo {
		<Test as Config>::Runner::call(
			H160::default(),
			to,
			calldata,
			U256::zero(),
			1_000_000,
			None, // max_fee_per_gas (non-transactional, no fee)
			None,
			None,
			Vec::new(),
			Vec::new(),
			false, // non-transactional (eth_call semantics)
			true,  // must be validated
			None,
			None,
			state_override,
			<Test as Config>::config(),
		)
		.expect("call succeeds")
	}

	fn assert_succeeded(info: &CallInfo) {
		assert_eq!(
			info.exit_reason,
			ExitReason::Succeed(ExitSucceed::Returned),
			"contract call must succeed",
		);
	}

	// --------------------------------------------------------------------
	// A2: state_override replaces persisted slot.
	// --------------------------------------------------------------------
	#[test]
	fn state_override_replaces_persisted_slot() {
		new_test_ext().execute_with(|| {
			let contract = H160::repeat_byte(0xA1);
			deploy_runtime_code(contract, SLOAD_RETURN_BYTECODE);

			// Seed on-chain storage: slot 1 -> 0x11.
			AccountStorages::<Test>::insert(contract, slot(1), value(0x11));

			// Control: with no override, SLOAD(1) reads the on-chain value.
			let baseline = eth_call_with_override(contract, slot_calldata(1), None);
			assert_succeeded(&baseline);
			assert_eq!(baseline.value, expected_return(0x11));

			// With override: SLOAD(1) reads the override value, not the on-chain one.
			let override_payload = Some(vec![(contract, vec![(slot(1), value(0x22))])]);
			let overridden = eth_call_with_override(contract, slot_calldata(1), override_payload);
			assert_succeeded(&overridden);
			assert_eq!(overridden.value, expected_return(0x22));

			// On-chain storage must be unchanged: eth_call does not commit.
			assert_eq!(
				AccountStorages::<Test>::get(contract, slot(1)),
				value(0x11),
				"persisted storage must not be mutated by an overridden eth_call",
			);
		});
	}

	// --------------------------------------------------------------------
	// A3: empty state_override wipes all slots (Geth `"state": {}` case).
	// --------------------------------------------------------------------
	//
	// This is the exact payload shape that previously forced the RPC layer to
	// enumerate every on-chain storage key for the target address (the DoS
	// vector). With the new runtime-API path, a full wipe costs O(1) in the
	// caller payload — the runtime simply marks the address as overridden with
	// no surviving slots.
	#[test]
	fn state_override_empty_wipes_all_slots() {
		new_test_ext().execute_with(|| {
			let contract = H160::repeat_byte(0xA2);
			deploy_runtime_code(contract, SLOAD_RETURN_BYTECODE);

			// Seed several on-chain slots.
			AccountStorages::<Test>::insert(contract, slot(1), value(0x11));
			AccountStorages::<Test>::insert(contract, slot(2), value(0x22));
			AccountStorages::<Test>::insert(contract, slot(3), value(0x33));

			// Full-wipe override: empty slot vec for this account.
			let wipe_override = Some(vec![(contract, Vec::<(H256, H256)>::new())]);

			for idx in [1u64, 2, 3, 99] {
				let info =
					eth_call_with_override(contract, slot_calldata(idx), wipe_override.clone());
				assert_succeeded(&info);
				assert_eq!(
					info.value,
					expected_return(0),
					"slot {idx} must read as zero under a full-wipe override",
				);
			}

			// Persisted storage must still be intact after the eth_call.
			assert_eq!(AccountStorages::<Test>::get(contract, slot(1)), value(0x11));
			assert_eq!(AccountStorages::<Test>::get(contract, slot(2)), value(0x22));
			assert_eq!(AccountStorages::<Test>::get(contract, slot(3)), value(0x33));
		});
	}

	// --------------------------------------------------------------------
	// A4: state_override is per-account.
	// --------------------------------------------------------------------
	//
	// Overriding account A must not affect reads of account B.
	#[test]
	fn state_override_is_per_account() {
		new_test_ext().execute_with(|| {
			let contract_a = H160::repeat_byte(0xA4);
			let contract_b = H160::repeat_byte(0xB4);
			deploy_runtime_code(contract_a, SLOAD_RETURN_BYTECODE);
			deploy_runtime_code(contract_b, SLOAD_RETURN_BYTECODE);

			AccountStorages::<Test>::insert(contract_a, slot(1), value(0x11));
			AccountStorages::<Test>::insert(contract_b, slot(1), value(0xAA));

			// Override only A.
			let override_payload = Some(vec![(contract_a, vec![(slot(1), value(0x22))])]);

			let info_a =
				eth_call_with_override(contract_a, slot_calldata(1), override_payload.clone());
			assert_succeeded(&info_a);
			assert_eq!(
				info_a.value,
				expected_return(0x22),
				"A must read the overridden value"
			);

			let info_b = eth_call_with_override(contract_b, slot_calldata(1), override_payload);
			assert_succeeded(&info_b);
			assert_eq!(
				info_b.value,
				expected_return(0xAA),
				"B must read on-chain storage, unaffected by A's override"
			);
		});
	}

	// --------------------------------------------------------------------
	// A5: missing slot in override reads zero (not the on-chain value).
	// --------------------------------------------------------------------
	//
	// This is the crux of the "destructed and recreated" semantics: once an
	// account is listed in the override map, *every* slot not present in that
	// map must read as zero, even if it has a non-zero on-chain value.
	#[test]
	fn state_override_missing_slot_reads_zero() {
		new_test_ext().execute_with(|| {
			let contract = H160::repeat_byte(0xA5);
			deploy_runtime_code(contract, SLOAD_RETURN_BYTECODE);

			// Seed two slots on chain.
			AccountStorages::<Test>::insert(contract, slot(1), value(0x11));
			AccountStorages::<Test>::insert(contract, slot(2), value(0xAA));

			// Override specifies only slot 1.
			let override_payload = Some(vec![(contract, vec![(slot(1), value(0x22))])]);

			let info_1 =
				eth_call_with_override(contract, slot_calldata(1), override_payload.clone());
			assert_succeeded(&info_1);
			assert_eq!(info_1.value, expected_return(0x22));

			let info_2 = eth_call_with_override(contract, slot_calldata(2), override_payload);
			assert_succeeded(&info_2);
			assert_eq!(
				info_2.value,
				expected_return(0),
				"slot 2 is absent from the override; must read zero despite on-chain 0xAA",
			);
		});
	}

	// --------------------------------------------------------------------
	// A6: SSTORE during the call wins over the override baseline.
	// --------------------------------------------------------------------
	#[test]
	fn sstore_during_call_wins_over_override() {
		new_test_ext().execute_with(|| {
			let contract = H160::repeat_byte(0xA6);
			deploy_runtime_code(contract, SSTORE_THEN_SLOAD_BYTECODE);

			// Override baseline: slot 1 -> 0x22.
			let override_payload = Some(vec![(contract, vec![(slot(1), value(0x22))])]);

			// Calldata says: SSTORE(slot=1, value=0x33); SLOAD(1); return.
			let info =
				eth_call_with_override(contract, slot_value_calldata(1, 0x33), override_payload);
			assert_succeeded(&info);
			assert_eq!(
				info.value,
				expected_return(0x33),
				"write made during the call must take precedence over the override baseline",
			);
		});
	}

	// --------------------------------------------------------------------
	// A7: state_override does not persist across calls.
	// --------------------------------------------------------------------
	//
	// Because eth_call is non-transactional, no state change (override or
	// SSTORE) should survive the call. We verify this with a direct on-chain
	// storage read and a second, override-less call.
	#[test]
	fn state_override_does_not_persist_across_calls() {
		new_test_ext().execute_with(|| {
			let contract = H160::repeat_byte(0xA7);
			deploy_runtime_code(contract, SSTORE_THEN_SLOAD_BYTECODE);

			AccountStorages::<Test>::insert(contract, slot(1), value(0x11));

			// First call: override + SSTORE a different value.
			let override_payload = Some(vec![(contract, vec![(slot(1), value(0x22))])]);
			let first =
				eth_call_with_override(contract, slot_value_calldata(1, 0x33), override_payload);
			assert_succeeded(&first);
			assert_eq!(first.value, expected_return(0x33));

			// On-chain storage must still hold the original seeded value.
			assert_eq!(
				AccountStorages::<Test>::get(contract, slot(1)),
				value(0x11),
				"non-transactional call must not mutate on-chain storage",
			);

			// Second call without an override: the original on-chain value must be visible.
			// We use the SLOAD-only contract for a clean read.
			let reader = H160::repeat_byte(0xA8);
			deploy_runtime_code(reader, SLOAD_RETURN_BYTECODE);
			AccountStorages::<Test>::insert(reader, slot(1), value(0x11));

			let second = eth_call_with_override(reader, slot_calldata(1), None);
			assert_succeeded(&second);
			assert_eq!(second.value, expected_return(0x11));
		});
	}

	// --------------------------------------------------------------------
	// A8: `None` state_override uses on-chain storage (regression guard).
	// --------------------------------------------------------------------
	//
	// A future refactor must not break the non-override path.
	#[test]
	fn none_state_override_uses_on_chain_storage() {
		new_test_ext().execute_with(|| {
			let contract = H160::repeat_byte(0xA9);
			deploy_runtime_code(contract, SLOAD_RETURN_BYTECODE);

			AccountStorages::<Test>::insert(contract, slot(7), value(0x777));

			let info = eth_call_with_override(contract, slot_calldata(7), None);
			assert_succeeded(&info);
			assert_eq!(info.value, expected_return(0x777));
		});
	}

	// --------------------------------------------------------------------
	// A9: `reset_storage` clears any active override entry.
	// --------------------------------------------------------------------
	//
	// Mirrors the Geth-parity guarantee for SELFDESTRUCT (or contract
	// re-creation): once an account is destructed, the previously overridden
	// storage is gone — subsequent reads fall back to the (now-empty) persisted
	// trie rather than the stale override snapshot.
	//
	// This is a direct backend-level test against `SubstrateStackState` so we
	// don't have to hand-craft a SELFDESTRUCT bytecode, which would also have
	// to navigate EIP-6780 semantics.
	#[test]
	fn reset_storage_clears_override_entry() {
		new_test_ext().execute_with(|| {
			let addr = H160::repeat_byte(0xD1);
			let slot_1 = slot(1);
			let val_1 = value(0x22);

			// Build a fresh SubstrateStackState with the override active.
			let vicinity = Vicinity {
				gas_price: U256::zero(),
				origin: H160::default(),
			};
			let config = <Test as Config>::config().clone();
			let metadata = evm::executor::stack::StackSubstateMetadata::new(1_000_000, &config);
			let state_override = Some(vec![(addr, vec![(slot_1, val_1)])]);
			let mut state = SubstrateStackState::<'_, '_, Test>::new(
				&vicinity,
				metadata,
				None,
				None,
				state_override,
			);

			// Before reset: override is active for this slot.
			assert_eq!(
				<SubstrateStackState<'_, '_, Test> as evm::backend::Backend>::storage(
					&state, addr, slot_1,
				),
				val_1,
				"override must be honored before reset_storage",
			);

			// SELFDESTRUCT-like: wipe the account's storage.
			<SubstrateStackState<'_, '_, Test> as evm::executor::stack::StackState<'_>>::reset_storage(
				&mut state, addr,
			);

			// After reset: override entry is cleared; subsequent reads fall back to
			// `AccountStorages`, which is empty for this address, so they return zero.
			assert_eq!(
				<SubstrateStackState<'_, '_, Test> as evm::backend::Backend>::storage(
					&state, addr, slot_1,
				),
				H256::zero(),
				"post-reset read must fall back to (empty) persisted storage and return zero",
			);
		});
	}
}
