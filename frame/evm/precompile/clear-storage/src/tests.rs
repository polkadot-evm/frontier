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

use core::str::from_utf8;

use crate::mock::{ExtBuilder, PCall, Precompiles, PrecompilesValue, Runtime};
use precompile_utils::testing::*;
use rlp::RlpStream;
use sp_core::{keccak_256, H160, H256};

pub fn contract_address(sender: H160, nonce: u64) -> H160 {
	let mut rlp = RlpStream::new_list(2);
	rlp.append(&sender);
	rlp.append(&nonce);

	H160::from_slice(&keccak_256(&rlp.out())[12..])
}

fn precompiles() -> Precompiles<Runtime> {
	PrecompilesValue::get()
}

fn mock_contract_with_entries(nonce: u64, num_entries: u32) -> H160 {
	let contract_address = contract_address(Alice.into(), nonce);

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

			precompiles()
				.prepare_test(
					Alice,
					Precompile1,
					PCall::clear_suicided_storage {
						addresses: vec![contract_address.into()].into(),
					},
				)
				.execute_reverts(|output| output == b"NotSuicided: {contract_address}");

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
