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

use crate::mock::{ExtBuilder, PCall, Precompiles, PrecompilesValue, Runtime};
use pallet_evm::AddressMapping;
use precompile_utils::testing::*;
use rlp::RlpStream;
use sp_core::{keccak_256, H160, H256};

// Helper function that calculates the contract address
pub fn contract_address(sender: H160, nonce: u64) -> H160 {
	let mut rlp = RlpStream::new_list(2);
	rlp.append(&sender);
	rlp.append(&nonce);

	H160::from_slice(&keccak_256(&rlp.out())[12..])
}

fn precompiles() -> Precompiles<Runtime> {
	PrecompilesValue::get()
}

// Helper function that creates a contract with `num_entries` storage entries
fn mock_contract_with_entries(nonce: u64, num_entries: u32) -> H160 {
	let contract_address = contract_address(Alice.into(), nonce);
	let account_id =
		<Runtime as pallet_evm::Config>::AddressMapping::into_account_id(contract_address);
	let _ = frame_system::Pallet::<Runtime>::inc_sufficients(&account_id);

	// Add num_entries storage entries to the suicided contract
	for i in 0..num_entries {
		pallet_evm::AccountStorages::<Runtime>::insert(
			contract_address,
			H256::from_low_u64_be(i as u64),
			H256::from_low_u64_be(i as u64),
		);
	}

	contract_address
}

#[test]
fn test_clear_suicided_contract_succesfull() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let contract_address = mock_contract_with_entries(1, 10);
			// Add contract to the suicided contracts
			pallet_evm::Suicided::<Runtime>::insert(contract_address, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![contract_address.into()].into(),
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address).count(),
				0
			);

			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address),
				false
			);
		})
}

// Test that the precompile fails if the contract is not suicided
#[test]
fn test_clear_suicided_contract_failed() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let contract_address = mock_contract_with_entries(1, 10);

			// Ensure that the contract is not suicided
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address),
				false
			);

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![contract_address.into()].into(),
					},
				)
				.execute_reverts(|output| {
					output == format!("NotSuicided: {}", contract_address).as_bytes()
				});

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address).count(),
				10
			);
		})
}

// Test that the precompile can handle an empty input
#[test]
fn test_clear_suicided_empty_input() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let contract_address = mock_contract_with_entries(1, 10);
			// Add contract to the suicided contracts
			pallet_evm::Suicided::<Runtime>::insert(contract_address, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![].into(),
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address).count(),
				10
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address),
				true
			);
		})
}

// Test with multiple suicided contracts ensuring that the precompile can handle multiple addresses at once.
#[test]
fn test_clear_suicided_contract_multiple_addresses() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let contract_address1 = mock_contract_with_entries(1, 10);
			let contract_address2 = mock_contract_with_entries(2, 20);
			let contract_address3 = mock_contract_with_entries(3, 30);

			// Add contract to the suicided contracts
			pallet_evm::Suicided::<Runtime>::insert(contract_address1, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address2, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address3, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![
							contract_address1.into(),
							contract_address2.into(),
							contract_address3.into(),
						]
						.into(),
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address1).count(),
				0
			);
			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address2).count(),
				0
			);
			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address3).count(),
				0
			);

			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address1),
				false
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address2),
				false
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address3),
				false
			);
		})
}

// Test a combination of Suicided and non-suicided contracts
#[test]
fn test_clear_suicided_mixed_suicided_and_non_suicided() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let contract_address1 = mock_contract_with_entries(1, 10);
			let contract_address2 = mock_contract_with_entries(2, 10);
			let contract_address3 = mock_contract_with_entries(3, 10);
			let contract_address4 = mock_contract_with_entries(4, 10);

			// Add contract to the suicided contracts
			pallet_evm::Suicided::<Runtime>::insert(contract_address1, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address2, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address4, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![
							contract_address1.into(),
							contract_address2.into(),
							contract_address3.into(),
							contract_address4.into(),
						]
						.into(),
					},
				)
				.execute_reverts(|output| {
					output == format!("NotSuicided: {}", contract_address3).as_bytes()
				});
		})
}

// Test that the precompile can handle suicided contracts that have no storage entries
#[test]
fn test_clear_suicided_no_storage_entries() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let contract_address1 = mock_contract_with_entries(1, 0);
			let contract_address2 = mock_contract_with_entries(1, 500);
			let contract_address3 = mock_contract_with_entries(1, 0);
			let contract_address4 = mock_contract_with_entries(1, 400);
			let contract_address5 = mock_contract_with_entries(1, 100);

			// Add contract to the suicided contracts
			pallet_evm::Suicided::<Runtime>::insert(contract_address1, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address2, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address3, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address4, ());
			pallet_evm::Suicided::<Runtime>::insert(contract_address5, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![contract_address1.into()].into(),
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address1).count(),
				0
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address1),
				false
			);
			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address2).count(),
				0
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address2),
				false
			);
			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address3).count(),
				0
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address3),
				false
			);
			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address4).count(),
				0
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address4),
				false
			);
			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address4).count(),
				0
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address4),
				false
			);
			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(contract_address5).count(),
				0
			);
			assert_eq!(
				pallet_evm::Suicided::<Runtime>::contains_key(contract_address5),
				false
			);
		})
}
