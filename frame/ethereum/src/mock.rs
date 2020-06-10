// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Frontier.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Test utilities

use super::*;
use crate::{Module, Trait};
use frame_support::{impl_outer_origin, parameter_types, weights::Weight};
use pallet_evm::{FeeCalculator, HashTruncateConvertAccountId};
use sp_core::{H160, H256, U256};
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    ModuleId, Perbill,
};
use std::str::FromStr;

impl_outer_origin! {
    pub enum Origin for Test where system = frame_system {}
}

// For testing the pallet, we construct most of a mock runtime. This means
// first constructing a configuration type (`Test`) which `impl`s each of the
// configuration traits of pallets we want to use.
#[derive(Clone, Eq, PartialEq)]
pub struct Test;
parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
}
impl frame_system::Trait for Test {
    type Origin = Origin;
    type Call = ();
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = H160;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = ();
    type BlockHashCount = BlockHashCount;
    type MaximumBlockWeight = MaximumBlockWeight;
    type DbWeight = ();
    type BlockExecutionWeight = ();
    type ExtrinsicBaseWeight = ();
    type MaximumExtrinsicWeight = MaximumBlockWeight;
    type MaximumBlockLength = MaximumBlockLength;
    type AvailableBlockRatio = AvailableBlockRatio;
    type Version = ();
    type ModuleToIndex = ();
    type AccountData = pallet_balances::AccountData<u64>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
}

parameter_types! {
    pub const ExistentialDeposit: u64 = 500;
}

impl pallet_balances::Trait for Test {
    type Balance = u64;
    type Event = ();
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
}

parameter_types! {
    pub const MinimumPeriod: u64 = 6000 / 2;
}

impl pallet_timestamp::Trait for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
}

pub struct FixedGasPrice;
impl FeeCalculator for FixedGasPrice {
    fn min_gas_price() -> U256 {
        1.into()
    }
}

parameter_types! {
    pub const TransactionByteFee: u64 = 1;
    pub const EVMModuleId: ModuleId = ModuleId(*b"py/evmpa");
}

impl pallet_evm::Trait for Test {
    type ModuleId = EVMModuleId;
    type FeeCalculator = FixedGasPrice;
    type ConvertAccountId = HashTruncateConvertAccountId<BlakeTwo256>;
    type Currency = Balances;
    type Event = ();
    type Precompiles = ();
}

impl Trait for Test {
    type Event = ();
}

pub type System = frame_system::Module<Test>;
pub type Balances = pallet_balances::Module<Test>;
pub type Ethereum = Module<Test>;
pub type Evm = pallet_evm::Module<Test>;

pub struct AccountInfo {
    pub address: H160,
    pub private_key: H256,
    pub r: H256,
    pub s: H256,
    pub v: u64,
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> (Vec<AccountInfo>, sp_io::TestExternalities) {
    let storage = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();
    let ext = storage.into();

    // TODO: Replace with a correct way to generate those.
    const ALICE_ACCOUNT: &str = "3725421C2ED3bB336a6Ebd4A06D2f3721679e47f";
    const ALICE_PRIVATE_KEY: &str =
        "f12a96d6ee380d41d87ba35e3d26dc1d8c8718b37578baae94c0460a5b26d3e3";
    const ALICE_V: u64 = 0x78;
    const ALICE_R: &str = "e5b8c39851a99396f1844171c99dd66a24f44df3c07c02925e3f547e653abc22";
    const ALICE_S: &str = "01570aba1ab55d32103e275f6b0ddda5d18a55b927f056e1eeb5238a77d004c1";

    let alice_account = H160::from_str(ALICE_ACCOUNT).unwrap();
    let alice_private_key = H256::from_str(ALICE_PRIVATE_KEY).unwrap();
    let alice_r = H256::from_str(ALICE_R).unwrap();
    let alice_s = H256::from_str(ALICE_S).unwrap();

    let pairs = vec![AccountInfo {
        address: alice_account,
        private_key: alice_private_key,
        r: alice_r,
        s: alice_s,
        v: ALICE_V,
    }];

    (pairs, ext)
}

pub fn contract_address(sender: H160, nonce: u64) -> H160 {
    let mut rlp = rlp::RlpStream::new_list(2);
    rlp.append(&sender);
    rlp.append(&nonce);

    H160::from_slice(&Keccak256::digest(rlp.out().as_slice())[12..])
}

pub fn storage_address(sender: H160, slot: H256) -> H256 {
    H256::from_slice(&Keccak256::digest(
        [&H256::from(sender)[..], &slot[..]].concat().as_slice(),
    ))
}
