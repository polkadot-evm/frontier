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

//! Test utilities

use core::str::FromStr;
use ethereum::{TransactionAction, legacy::TransactionSignature as LegacyTransactionSignature, eip2930::TransactionSignature as EIP2930TransactionSignature};
use rlp::RlpStream;
// Substrate
use frame_support::{derive_impl, parameter_types, traits::FindAuthor, ConsensusEngineId};
use sp_core::{hashing::keccak_256, H160, H256, U256};
use sp_runtime::{
	traits::{Dispatchable, IdentityLookup},
	AccountId32, BuildStorage,
};
// Frontier
use pallet_evm::{config_preludes::ChainId, AddressMapping, EnsureAllowedCreateAddress};

use super::*;

pub type SignedExtra = (frame_system::CheckSpecVersion<Test>,);

frame_support::construct_runtime! {
	pub enum Test {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage},
		EVM: pallet_evm::{Pallet, Call, Storage, Config<T>, Event<T>},
		Ethereum: crate::{Pallet, Call, Storage, Event, Origin},
	}
}

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = frame_system::mocking::MockBlock<Self>;
	type BlockHashCount = BlockHashCount;
	type AccountData = pallet_balances::AccountData<u64>;
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 0;
	// For weight estimation, we assume that the most locks on an individual account will be 50.
	// This number may need to be adjusted in the future if this assumption no longer holds true.
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {}

pub struct FindAuthorTruncated;
impl FindAuthor<H160> for FindAuthorTruncated {
	fn find_author<'a, I>(_digests: I) -> Option<H160>
	where
		I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
	{
		Some(address_build(0).address)
	}
}

parameter_types! {
	pub const TransactionByteFee: u64 = 1;
	pub const GasLimitStorageGrowthRatio: u64 = 0;
	// Alice is allowed to create contracts via CREATE and CALL(CREATE)
	pub AllowedAddressesCreate: Vec<H160> = vec![H160::from_str("0x1a642f0e3c3af545e7acbd38b07251b3990914f1").expect("alice address")];
	pub AllowedAddressesCreateInner: Vec<H160> = vec![H160::from_str("0x1a642f0e3c3af545e7acbd38b07251b3990914f1").expect("alice address")];
}

#[derive_impl(pallet_evm::config_preludes::TestDefaultConfig)]
impl pallet_evm::Config for Test {
	type AccountProvider = pallet_evm::FrameSystemAccountProvider<Self>;
	type BlockHashMapping = crate::EthereumBlockHashMapping<Self>;
	type CreateOriginFilter = EnsureAllowedCreateAddress<AllowedAddressesCreate>;
	type CreateInnerOriginFilter = EnsureAllowedCreateAddress<AllowedAddressesCreateInner>;
	type Currency = Balances;
	type PrecompilesType = ();
	type PrecompilesValue = ();
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type FindAuthor = FindAuthorTruncated;
	type GasLimitStorageGrowthRatio = GasLimitStorageGrowthRatio;
	type Timestamp = Timestamp;
}

#[derive_impl(crate::config_preludes::TestDefaultConfig)]
impl Config for Test {}

impl fp_self_contained::SelfContainedCall for RuntimeCall {
	type SignedInfo = H160;

	fn is_self_contained(&self) -> bool {
		match self {
			RuntimeCall::Ethereum(call) => call.is_self_contained(),
			_ => false,
		}
	}

	fn check_self_contained(&self) -> Option<Result<Self::SignedInfo, TransactionValidityError>> {
		match self {
			RuntimeCall::Ethereum(call) => call.check_self_contained(),
			_ => None,
		}
	}

	fn validate_self_contained(
		&self,
		info: &Self::SignedInfo,
		dispatch_info: &DispatchInfoOf<RuntimeCall>,
		len: usize,
	) -> Option<TransactionValidity> {
		match self {
			RuntimeCall::Ethereum(call) => call.validate_self_contained(info, dispatch_info, len),
			_ => None,
		}
	}

	fn pre_dispatch_self_contained(
		&self,
		info: &Self::SignedInfo,
		dispatch_info: &DispatchInfoOf<RuntimeCall>,
		len: usize,
	) -> Option<Result<(), TransactionValidityError>> {
		match self {
			RuntimeCall::Ethereum(call) => {
				call.pre_dispatch_self_contained(info, dispatch_info, len)
			}
			_ => None,
		}
	}

	fn apply_self_contained(
		self,
		info: Self::SignedInfo,
	) -> Option<sp_runtime::DispatchResultWithInfo<sp_runtime::traits::PostDispatchInfoOf<Self>>> {
		match self {
			call @ RuntimeCall::Ethereum(crate::Call::transact { .. }) => {
				Some(call.dispatch(RuntimeOrigin::from(RawOrigin::EthereumTransaction(info))))
			}
			_ => None,
		}
	}
}

pub struct AccountInfo {
	pub address: H160,
	pub account_id: AccountId32,
	pub private_key: H256,
}

fn address_build(seed: u8) -> AccountInfo {
	let private_key = H256::from_slice(&[(seed + 1); 32]); //H256::from_low_u64_be((i + 1) as u64);
	let secret_key = libsecp256k1::SecretKey::parse_slice(&private_key[..]).unwrap();
	let public_key = &libsecp256k1::PublicKey::from_secret_key(&secret_key).serialize()[1..65];
	let address = H160::from(H256::from(keccak_256(public_key)));

	AccountInfo {
		private_key,
		account_id: <Test as pallet_evm::Config>::AddressMapping::into_account_id(address),
		address,
	}
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext(accounts_len: usize) -> (Vec<AccountInfo>, sp_io::TestExternalities) {
	// sc_cli::init_logger("");
	let mut ext = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();

	let pairs = (0..accounts_len)
		.map(|i| address_build(i as u8))
		.collect::<Vec<_>>();

	let balances: Vec<_> = (0..accounts_len)
		.map(|i| (pairs[i].account_id.clone(), 10_000_000))
		.collect();

	pallet_balances::GenesisConfig::<Test> {
		balances,
		dev_accounts: None,
	}
	.assimilate_storage(&mut ext)
	.unwrap();

	(pairs, ext.into())
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext_with_initial_balance(
	accounts_len: usize,
	initial_balance: u64,
) -> (Vec<AccountInfo>, sp_io::TestExternalities) {
	let mut ext = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();

	let pairs = (0..accounts_len)
		.map(|i| address_build(i as u8))
		.collect::<Vec<_>>();

	let balances: Vec<_> = (0..accounts_len)
		.map(|i| (pairs[i].account_id.clone(), initial_balance))
		.collect();

	pallet_balances::GenesisConfig::<Test> {
		balances,
		dev_accounts: None,
	}
	.assimilate_storage(&mut ext)
	.unwrap();

	(pairs, ext.into())
}

pub fn contract_address(sender: H160, nonce: u64) -> H160 {
	let mut rlp = RlpStream::new_list(2);
	rlp.append(&sender);
	rlp.append(&nonce);

	H160::from_slice(&keccak_256(&rlp.out())[12..])
}

pub fn storage_address(sender: H160, slot: H256) -> H256 {
	H256::from(keccak_256(
		[&H256::from(sender)[..], &slot[..]].concat().as_slice(),
	))
}

pub struct LegacyUnsignedTransaction {
	pub nonce: U256,
	pub gas_price: U256,
	pub gas_limit: U256,
	pub action: TransactionAction,
	pub value: U256,
	pub input: Vec<u8>,
}

impl LegacyUnsignedTransaction {
	fn signing_rlp_append(&self, s: &mut RlpStream) {
		s.begin_list(9);
		s.append(&self.nonce);
		s.append(&self.gas_price);
		s.append(&self.gas_limit);
		s.append(&self.action);
		s.append(&self.value);
		s.append(&self.input);
		s.append(&ChainId::get());
		s.append(&0u8);
		s.append(&0u8);
	}

	fn signing_hash(&self) -> H256 {
		let mut stream = RlpStream::new();
		self.signing_rlp_append(&mut stream);
		H256::from(keccak_256(&stream.out()))
	}

	pub fn sign(&self, key: &H256) -> Transaction {
		self.sign_with_chain_id(key, ChainId::get())
	}

	pub fn sign_with_chain_id(&self, key: &H256, chain_id: u64) -> Transaction {
		let hash = self.signing_hash();
		let msg = libsecp256k1::Message::parse(hash.as_fixed_bytes());
		let s = libsecp256k1::sign(
			&msg,
			&libsecp256k1::SecretKey::parse_slice(&key[..]).unwrap(),
		);
		let sig = s.0.serialize();

		let sig = LegacyTransactionSignature::new(
			s.1.serialize() as u64 % 2 + chain_id * 2 + 35,
			H256::from_slice(&sig[0..32]),
			H256::from_slice(&sig[32..64]),
		)
		.unwrap();

		Transaction::Legacy(ethereum::LegacyTransaction {
			nonce: self.nonce,
			gas_price: self.gas_price,
			gas_limit: self.gas_limit,
			action: self.action,
			value: self.value,
			input: self.input.clone(),
			signature: sig,
		})
	}
}

pub struct EIP2930UnsignedTransaction {
	pub nonce: U256,
	pub gas_price: U256,
	pub gas_limit: U256,
	pub action: TransactionAction,
	pub value: U256,
	pub input: Vec<u8>,
}

impl EIP2930UnsignedTransaction {
	pub fn sign(&self, secret: &H256, chain_id: Option<u64>) -> Transaction {
		let secret = {
			let mut sk: [u8; 32] = [0u8; 32];
			sk.copy_from_slice(&secret[0..]);
			libsecp256k1::SecretKey::parse(&sk).unwrap()
		};
		let chain_id = chain_id.unwrap_or(ChainId::get());
		let msg = ethereum::EIP2930TransactionMessage {
			chain_id,
			nonce: self.nonce,
			gas_price: self.gas_price,
			gas_limit: self.gas_limit,
			action: self.action,
			value: self.value,
			input: self.input.clone(),
			access_list: vec![],
		};
		let signing_message = libsecp256k1::Message::parse_slice(&msg.hash()[..]).unwrap();

		let (signature, recid) = libsecp256k1::sign(&signing_message, &secret);
		let rs = signature.serialize();
		let r = H256::from_slice(&rs[0..32]);
		let s = H256::from_slice(&rs[32..64]);
		Transaction::EIP2930(ethereum::EIP2930Transaction {
			chain_id: msg.chain_id,
			nonce: msg.nonce,
			gas_price: msg.gas_price,
			gas_limit: msg.gas_limit,
			action: msg.action,
			value: msg.value,
			input: msg.input.clone(),
			access_list: msg.access_list,
			signature: EIP2930TransactionSignature::new(
				recid.serialize() != 0,
				r,
				s,
			).unwrap(),
		})
	}
}

pub struct EIP1559UnsignedTransaction {
	pub nonce: U256,
	pub max_priority_fee_per_gas: U256,
	pub max_fee_per_gas: U256,
	pub gas_limit: U256,
	pub action: TransactionAction,
	pub value: U256,
	pub input: Vec<u8>,
}

impl EIP1559UnsignedTransaction {
	pub fn sign(&self, secret: &H256, chain_id: Option<u64>) -> Transaction {
		let secret = {
			let mut sk: [u8; 32] = [0u8; 32];
			sk.copy_from_slice(&secret[0..]);
			libsecp256k1::SecretKey::parse(&sk).unwrap()
		};
		let chain_id = chain_id.unwrap_or(ChainId::get());
		let msg = ethereum::EIP1559TransactionMessage {
			chain_id,
			nonce: self.nonce,
			max_priority_fee_per_gas: self.max_priority_fee_per_gas,
			max_fee_per_gas: self.max_fee_per_gas,
			gas_limit: self.gas_limit,
			action: self.action,
			value: self.value,
			input: self.input.clone(),
			access_list: vec![],
		};
		let signing_message = libsecp256k1::Message::parse_slice(&msg.hash()[..]).unwrap();

		let (signature, recid) = libsecp256k1::sign(&signing_message, &secret);
		let rs = signature.serialize();
		let r = H256::from_slice(&rs[0..32]);
		let s = H256::from_slice(&rs[32..64]);
		Transaction::EIP1559(ethereum::EIP1559Transaction {
			chain_id: msg.chain_id,
			nonce: msg.nonce,
			max_priority_fee_per_gas: msg.max_priority_fee_per_gas,
			max_fee_per_gas: msg.max_fee_per_gas,
			gas_limit: msg.gas_limit,
			action: msg.action,
			value: msg.value,
			input: msg.input.clone(),
			access_list: msg.access_list,
			signature: EIP2930TransactionSignature::new(
				recid.serialize() != 0,
				r,
				s,
			).unwrap(),
		})
	}
}

pub struct EIP7702UnsignedTransaction {
	pub nonce: U256,
	pub max_priority_fee_per_gas: U256,
	pub max_fee_per_gas: U256,
	pub gas_limit: U256,
	pub destination: TransactionAction,
	pub value: U256,
	pub data: Vec<u8>,
	pub authorization_list: Vec<ethereum::AuthorizationListItem>,
}

impl EIP7702UnsignedTransaction {
	pub fn sign(&self, secret: &H256, chain_id: Option<u64>) -> Transaction {
		let secret = {
			let mut sk: [u8; 32] = [0u8; 32];
			sk.copy_from_slice(&secret[0..]);
			libsecp256k1::SecretKey::parse(&sk).unwrap()
		};
		let chain_id = chain_id.unwrap_or(ChainId::get());
		let msg = ethereum::EIP7702TransactionMessage {
			chain_id,
			nonce: self.nonce,
			max_priority_fee_per_gas: self.max_priority_fee_per_gas,
			max_fee_per_gas: self.max_fee_per_gas,
			gas_limit: self.gas_limit,
			destination: self.destination,
			value: self.value,
			data: self.data.clone(),
			access_list: vec![],
			authorization_list: self.authorization_list.clone(),
		};
		let signing_message = libsecp256k1::Message::parse_slice(&msg.hash()[..]).unwrap();

		let (signature, recid) = libsecp256k1::sign(&signing_message, &secret);
		let rs = signature.serialize();
		let r = H256::from_slice(&rs[0..32]);
		let s = H256::from_slice(&rs[32..64]);
		Transaction::EIP7702(ethereum::EIP7702Transaction {
			chain_id: msg.chain_id,
			nonce: msg.nonce,
			max_priority_fee_per_gas: msg.max_priority_fee_per_gas,
			max_fee_per_gas: msg.max_fee_per_gas,
			gas_limit: msg.gas_limit,
			destination: msg.destination,
			value: msg.value,
			data: msg.data.clone(),
			access_list: msg.access_list,
			authorization_list: msg.authorization_list,
			signature: EIP2930TransactionSignature::new(
				recid.serialize() != 0,
				r,
				s,
			).unwrap(),
		})
	}
}
