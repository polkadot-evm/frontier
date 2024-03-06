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

//! Test mock for unit tests.

use frame_support::traits::{ConstU32, ConstU64};
use mockall::{mock, predicate::*};
use sp_core::{H160, H256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};
use sp_std::{boxed::Box, prelude::*};

use crate::{self as pallet_evm_system, *};

mock! {
	#[derive(Debug)]
	pub DummyOnNewAccount {}

	impl OnNewAccount<H160> for DummyOnNewAccount {
		pub fn on_new_account(who: &H160);
	}
}

mock! {
	#[derive(Debug)]
	pub DummyOnKilledAccount {}

	impl OnKilledAccount<H160> for DummyOnKilledAccount {
		pub fn on_killed_account(who: &H160);
	}
}

frame_support::construct_runtime! {
	pub enum Test {
		System: frame_system,
		EvmSystem: pallet_evm_system,
	}
}

impl frame_system::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = RuntimeTask;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = H160;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = frame_system::mocking::MockBlock<Self>;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

impl pallet_evm_system::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AccountId = H160;
	type Nonce = u64;
	type AccountData = u64;
	type OnNewAccount = MockDummyOnNewAccount;
	type OnKilledAccount = MockDummyOnKilledAccount;
}

/// Build test externalities from the custom genesis.
/// Using this call requires manual assertions on the genesis init logic.
pub fn new_test_ext() -> sp_io::TestExternalities {
	// Build genesis.
	let config = RuntimeGenesisConfig {
		..Default::default()
	};
	let storage = config.build_storage().unwrap();

	// Make test externalities from the storage.
	storage.into()
}

pub fn runtime_lock() -> std::sync::MutexGuard<'static, ()> {
	static MOCK_RUNTIME_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

	// Ignore the poisoning for the tests that panic.
	// We only care about concurrency here, not about the poisoning.
	match MOCK_RUNTIME_MUTEX.lock() {
		Ok(guard) => guard,
		Err(poisoned) => poisoned.into_inner(),
	}
}

pub trait TestExternalitiesExt {
	fn execute_with_ext<R, E>(&mut self, execute: E) -> R
	where
		E: for<'e> FnOnce(&'e ()) -> R;
}

impl TestExternalitiesExt for sp_io::TestExternalities {
	fn execute_with_ext<R, E>(&mut self, execute: E) -> R
	where
		E: for<'e> FnOnce(&'e ()) -> R,
	{
		let guard = runtime_lock();
		let result = self.execute_with(|| execute(&guard));
		drop(guard);
		result
	}
}
