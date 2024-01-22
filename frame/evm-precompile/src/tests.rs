// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2023 Parity Technologies (UK) Ltd.
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

#![cfg(test)]

use super::*;
use crate::mock::*;

use fp_evm::GenesisAccount;
use frame_support::{assert_ok, traits::GenesisBuild};
use sp_core::U256;
use std::{collections::BTreeMap, str::FromStr};

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default()
		.build_storage::<Test>()
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
	.assimilate_storage(&mut t)
	.expect("Pallet balances storage can be assimilated");
	GenesisBuild::<Test>::assimilate_storage(&pallet_evm::GenesisConfig { accounts }, &mut t)
		.unwrap();
	t.into()
}

#[test]
fn precompile_storage_works() {
	new_test_ext().execute_with(|| {
		let origin = RuntimeOrigin::root();
		let address = H160::from_low_u64_be(1);

		let mut read_precompile = Pallet::<Test>::precompiles(address);
		assert_eq!(read_precompile, PrecompileLabel::default(),);

		let label = PrecompileLabel::new(
			b"ECRecover"
				.to_vec()
				.try_into()
				.expect("less than 32 chars; qed"),
		);

		assert_ok!(Pallet::<Test>::add_precompile(
			origin.clone(),
			address,
			label.clone(),
		));

		read_precompile = Pallet::<Test>::precompiles(address);
		assert_eq!(read_precompile, label);

		assert_ok!(Pallet::<Test>::remove_precompile(origin, address));

		read_precompile = Pallet::<Test>::precompiles(address);
		assert_eq!(read_precompile, PrecompileLabel::default());
	});
}
