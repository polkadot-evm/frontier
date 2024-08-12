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

use crate::mock::{ExtBuilder, PCall, Precompiles, PrecompilesValue, Runtime};
use pallet_evm::AddressMapping;
use precompile_utils::{solidity::codec::Address, testing::*};
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

// Helper function that creates an account. Returns the address of the account
fn mock_account(nonce: u64) -> H160 {
	let address = contract_address(Alice.into(), nonce);
	let account_id = <Runtime as pallet_evm::Config>::AddressMapping::into_account_id(address);
	let _ = frame_system::Pallet::<Runtime>::inc_sufficients(&account_id);
	address
}

// Helper function that creates storage entries for a contract
fn mock_entries(address: H160, num_entries: u32) {
	for i in 0..num_entries {
		pallet_evm::AccountStorages::<Runtime>::insert(
			address,
			H256::from_low_u64_be(i as u64),
			H256::from_low_u64_be(i as u64),
		);
	}
}

// Helper function that creates contracts with storage entries. Returns contract addresses
fn mock_contracts(entries: impl IntoIterator<Item = u32>) -> Vec<Address> {
	entries
		.into_iter()
		.enumerate()
		.map(|(i, j)| {
			let address = mock_account(i as u64);
			mock_entries(address, j);
			Address(address)
		})
		.collect()
}

#[test]
fn test_clear_suicided_contract_succesfull() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let suicided_address = mock_contracts([10])[0].0;
			pallet_evm::Suicided::<Runtime>::insert(suicided_address, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![suicided_address.into()].into(),
						limit: u64::MAX,
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(suicided_address).count(),
				0
			);
			assert!(!pallet_evm::Suicided::<Runtime>::contains_key(
				suicided_address
			));
		})
}

// Test that the precompile fails if the contract is not suicided
#[test]
fn test_clear_suicided_contract_failed() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let addresses = mock_contracts([10]);
			let non_suicided_address = addresses[0].0;

			// Ensure that the contract is not suicided
			assert!(!pallet_evm::Suicided::<Runtime>::contains_key(
				non_suicided_address
			));

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![non_suicided_address.into()].into(),
						limit: u64::MAX,
					},
				)
				.execute_reverts(|output| {
					output == format!("NotSuicided: {}", non_suicided_address).as_bytes()
				});

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(non_suicided_address).count(),
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
			let addresses = mock_contracts([10]);
			// Add contract to the suicided contracts
			pallet_evm::Suicided::<Runtime>::insert(addresses[0].0, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![].into(),
						limit: u64::MAX,
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(addresses[0].0).count(),
				10
			);
			assert!(pallet_evm::Suicided::<Runtime>::contains_key(
				addresses[0].0
			));
		})
}

// Test with multiple suicided contracts ensuring that the precompile can handle multiple addresses at once.
#[test]
fn test_clear_suicided_contract_multiple_addresses() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let addresses = mock_contracts([10, 20, 30]);

			for address in &addresses {
				pallet_evm::Suicided::<Runtime>::insert(address.0, ());
			}

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: addresses.clone().into(),
						limit: u64::MAX,
					},
				)
				.execute_returns(());

			for Address(address) in addresses {
				assert_eq!(
					pallet_evm::AccountStorages::<Runtime>::iter_prefix(address).count(),
					0
				);
				assert!(!pallet_evm::Suicided::<Runtime>::contains_key(address));
			}
		})
}

// Test a combination of Suicided and non-suicided contracts
#[test]
fn test_clear_suicided_mixed_suicided_and_non_suicided() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let addresses = mock_contracts([10, 20, 30, 10]);

			// Add contract to the suicided contracts
			(0..3).for_each(|i| {
				pallet_evm::Suicided::<Runtime>::insert(addresses[i].0, ());
			});

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: addresses.clone().into(),
						limit: u64::MAX,
					},
				)
				.execute_reverts(|output| {
					output == format!("NotSuicided: {}", addresses[3].0).as_bytes()
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
			let num_entries = [0, 500, 0, 400, 100];
			let addresses = mock_contracts(num_entries);

			for Address(address) in &addresses {
				pallet_evm::Suicided::<Runtime>::insert(address, ());
			}

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: addresses.clone().into(),
						limit: u64::MAX,
					},
				)
				.execute_returns(());

			for Address(address) in addresses {
				assert_eq!(
					pallet_evm::AccountStorages::<Runtime>::iter_prefix(address).count(),
					0
				);
				assert!(!pallet_evm::Suicided::<Runtime>::contains_key(address));
			}
		})
}

// Test that the precompile deletes entries up to the limit
#[test]
fn test_clear_suicided_contract_limit_works() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let addresses = mock_contracts([3, 4]);
			// Add contract to the suicided contracts
			for Address(address) in &addresses {
				pallet_evm::Suicided::<Runtime>::insert(address, ());
			}

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: addresses.clone().into(),
						limit: 5,
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(addresses[0].0).count(),
				0
			);
			assert!(!pallet_evm::Suicided::<Runtime>::contains_key(
				addresses[0].0
			));

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(addresses[1].0).count(),
				2
			);

			assert!(pallet_evm::Suicided::<Runtime>::contains_key(
				addresses[1].0
			));
		})
}

#[test]
fn test_clear_suicided_contract_limit_respected() {
	ExtBuilder::default()
		.with_balances(vec![(Alice.into(), 10000000000000000000)])
		.build()
		.execute_with(|| {
			let suicided_address = mock_contracts([6])[0].0;
			// Add contract to the suicided contracts
			pallet_evm::Suicided::<Runtime>::insert(suicided_address, ());

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![suicided_address.into()].into(),
						limit: 5,
					},
				)
				.execute_returns(());

			assert_eq!(
				pallet_evm::AccountStorages::<Runtime>::iter_prefix(suicided_address).count(),
				1
			);
			assert!(pallet_evm::Suicided::<Runtime>::contains_key(
				suicided_address
			));
		})
}
