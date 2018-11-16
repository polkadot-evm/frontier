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
use node_primitives::AccountId;
use node_runtime::{
	GenesisConfig, ConsensusConfig, SessionConfig, TimestampConfig, UpgradeKeyConfig,
	SystemConfig, EthereumConfig
};
use substrate_service;

/// Specialised `ChainSpec`.
pub type ChainSpec = substrate_service::ChainSpec<GenesisConfig>;

fn testnet_genesis(initial_authorities: Vec<AuthorityId>, upgrade_key: AccountId) -> GenesisConfig {
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
