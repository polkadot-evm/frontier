// Copyright 2018 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

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

//! Substrate chain configurations.

use std::marker::PhantomData;
use primitives::{AuthorityId, ed25519};
use node_primitives::{AccountId, H256, H160, U256};
use node_runtime::{
	GenesisConfig, ConsensusConfig, SessionConfig, TimestampConfig, UpgradeKeyConfig,
	SystemConfig, EthereumConfig,
};
use node_runtime::ethereum::BasicAccount;
use substrate_service;
use ethjson::spec::Spec;
use std::collections::BTreeMap;

/// Specialised `ChainSpec`.
pub type ChainSpec = substrate_service::ChainSpec<GenesisConfig>;

fn testnet_genesis(initial_authorities: Vec<AuthorityId>, upgrade_key: AccountId) -> GenesisConfig {
	let keccak_empty = H256::from([0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0, 0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70]);
	let keccak_null_rlp = H256::from([0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6, 0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e, 0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0, 0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21]);

	let ethspec = Spec::load(include_bytes!("../res/foundation.json").as_ref()).unwrap();
	let mut accounts: BTreeMap<H160, BasicAccount> = BTreeMap::default();

	for (address, account) in ethspec.accounts {
		if account.builtin.is_some() {
			continue
		}

		let address = H160::from(address);
		let account = BasicAccount {
			nonce: U256::zero(),
			balance: account.balance.unwrap().into(),
			storage_root: keccak_null_rlp,
			code_hash: keccak_empty,
		};
		accounts.insert(address, account);
	}

	GenesisConfig {
		consensus: Some(ConsensusConfig {
			code: include_bytes!("../../runtime/wasm/target/wasm32-unknown-unknown/release/node_runtime.compact.wasm").to_vec(),
			authorities: initial_authorities.clone(),
		}),
		system: Some(SystemConfig {
			_phantom: PhantomData,
			changes_trie_config: None,
		}),
		ethereum: Some(EthereumConfig {
			accounts: accounts,
			_phantom: PhantomData,
		}),
		session: Some(SessionConfig {
			validators: initial_authorities.iter().cloned().map(Into::into).collect(),
			session_length: 10,
		}),
		timestamp: Some(TimestampConfig {
			period: 5,					// 5 second block time.
		}),
		upgrade_key: Some(UpgradeKeyConfig {
			key: upgrade_key,
		}),
	}
}

fn development_config_genesis() -> GenesisConfig {
	testnet_genesis(vec![
		ed25519::Pair::from_seed(b"Alice                           ").public().into(),
	],
		ed25519::Pair::from_seed(b"Alice                           ").public().0.into()
	)
}

/// Development config (single validator Alice)
pub fn development_config() -> ChainSpec {
	ChainSpec::from_genesis("Development", "development", development_config_genesis, vec![], None, None, None)
}

fn local_testnet_genesis() -> GenesisConfig {
	testnet_genesis(vec![
		ed25519::Pair::from_seed(b"Alice                           ").public().into(),
		ed25519::Pair::from_seed(b"Bob                             ").public().into(),
	],
		ed25519::Pair::from_seed(b"Alice                           ").public().0.into()
	)
}

/// Local testnet config (multivalidator Alice + Bob)
pub fn local_testnet_config() -> ChainSpec {
	ChainSpec::from_genesis("Local Testnet", "local_testnet", local_testnet_genesis, vec![], None, None, None)
}

#[cfg(test)]
mod tests {
	use super::*;
	use service_test;
	use service::Factory;

	fn local_testnet_genesis_instant() -> GenesisConfig {
		let mut genesis = local_testnet_genesis();
		genesis.timestamp = Some(TimestampConfig { period: 0 });
		genesis
	}

	/// Local testnet config (multivalidator Alice + Bob)
	pub fn integration_test_config() -> ChainSpec {
		ChainSpec::from_genesis("Integration Test", "test", local_testnet_genesis_instant, vec![], None, None, None)
	}

	#[test]
	fn test_connectivity() {
		service_test::connectivity::<Factory>(integration_test_config());
	}
}
