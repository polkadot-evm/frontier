use std::{collections::BTreeMap, str::FromStr};

use hex_literal::hex;
// Substrate
use sc_chain_spec::{ChainType, Properties};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{Pair, Public, H160, U256};
use sp_runtime::traits::{IdentifyAccount, Verify};

// Tokfin runtime types
use tokfin_runtime::{AccountId, Balance, SS58Prefix, Signature, WASM_BINARY};

/// Specialized `ChainSpec` using JSON patch.
pub type ChainSpec = sc_service::GenericChainSpec;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

#[allow(dead_code)]
type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account ID from seed.
#[allow(dead_code)]
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Generate Aura/Grandpa authority keys from seed name.
pub fn authority_keys_from_seed(s: &str) -> (AuraId, GrandpaId) {
    (get_from_seed::<AuraId>(s), get_from_seed::<GrandpaId>(s))
}

fn properties() -> Properties {
    let mut properties = Properties::new();
    properties.insert("tokenSymbol".into(), "TKF".into());
    properties.insert("tokenDecimals".into(), 18.into());
    properties.insert("ss58Format".into(), SS58Prefix::get().into());
    properties
}

const UNITS: Balance = 1_000_000_000_000_000_000; // 10^18

/// Development chain spec (one authority).
pub fn development_config(enable_manual_seal: bool) -> ChainSpec {
    ChainSpec::builder(WASM_BINARY.expect("WASM not available"), Default::default())
        .with_name("Development")
        .with_id("dev")
        .with_chain_type(ChainType::Development)
        .with_properties(properties())
        .with_genesis_config_patch(testnet_genesis_json(
            // Sudo account (Alith)
            AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
            // Prefunded accounts
            vec![
                AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")), // Alith
                AccountId::from(hex!("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")), // Baltathar
                AccountId::from(hex!("798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc")), // Charleth
                AccountId::from(hex!("773539d4Ac0e786233D90A233654ccEE26a613D9")), // Dorothy
                AccountId::from(hex!("Ff64d3F6efE2317EE2807d223a0Bdc4c0c49dfDB")), // Ethan
                AccountId::from(hex!("C0F0f4ab324C46e55D02D0033343B4Be8A55532d")), // Faith
            ],
            // Initial PoA authorities
            vec![authority_keys_from_seed("Alice")],
            // EVM chain ID
            SS58Prefix::get() as u64,
            enable_manual_seal,
        ))
        .build()
}

/// Local testnet (two authorities).
pub fn local_testnet_config() -> ChainSpec {
    ChainSpec::builder(WASM_BINARY.expect("WASM not available"), Default::default())
        .with_name("Local Testnet")
        .with_id("local_testnet")
        .with_chain_type(ChainType::Local)
        .with_properties(properties())
        .with_genesis_config_patch(testnet_genesis_json(
            // Sudo account (Alith)
            AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
            // Prefunded accounts
            vec![
                AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")), // Alith
                AccountId::from(hex!("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")), // Baltathar
                AccountId::from(hex!("798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc")), // Charleth
                AccountId::from(hex!("773539d4Ac0e786233D90A233654ccEE26a613D9")), // Dorothy
                AccountId::from(hex!("Ff64d3F6efE2317EE2807d223a0Bdc4c0c49dfDB")), // Ethan
                AccountId::from(hex!("C0F0f4ab324C46e55D02D0033343B4Be8A55532d")), // Faith
            ],
            // Authorities
            vec![
                authority_keys_from_seed("Alice"),
                authority_keys_from_seed("Bob"),
            ],
            42,     // EVM chain ID
            false,  // manual seal disabled
        ))
        .build()
}

/// Build the JSON patch for genesis.
/// NOTE: keys use the same camelCase as in other pallets (evmChainId, manualSeal, etc.)
fn testnet_genesis_json(
    sudo_key: AccountId,
    endowed_accounts: Vec<AccountId>,
    initial_authorities: Vec<(AuraId, GrandpaId)>,
    chain_id: u64,
    enable_manual_seal: bool,
) -> serde_json::Value {
    // EVM precompiles/accounts example
    let evm_accounts = {
        let mut map = BTreeMap::new();
        map.insert(
            H160::from_str("d43593c715fdd31c61141abd04a99fd6822c8558")
                .expect("valid H160; qed"),
            fp_evm::GenesisAccount {
                balance: U256::from_str("0xffffffffffffffffffffffffffffffff")
                    .expect("valid U256; qed"),
                code: Default::default(),
                nonce: Default::default(),
                storage: Default::default(),
            },
        );
        map.insert(
            H160::from_str("6be02d1d3665660d22ff9624b7be0551ee1ac91b")
                .expect("valid H160; qed"),
            fp_evm::GenesisAccount {
                balance: U256::from_str("0xffffffffffffffffffffffffffffffff")
                    .expect("valid U256; qed"),
                code: Default::default(),
                nonce: Default::default(),
                storage: Default::default(),
            },
        );
        map.insert(
            H160::from_str("1000000000000000000000000000000000000001")
                .expect("valid H160; qed"),
            fp_evm::GenesisAccount {
                nonce: U256::from(1),
                balance: U256::from(1_000_000_000_000_000_000_000_000u128),
                storage: Default::default(),
                code: vec![0x00],
            },
        );
        map
    };

    // Initial balances
    let balances: Vec<(AccountId, Balance)> = endowed_accounts
        .iter()
        .cloned()
        .map(|acc| (acc, 1_000 * UNITS))
        .collect();

    // Tokfin assets (pallet-assets instanced as TokfinAssets in the runtime)
    // - 1: TKFr (Reputation)
    // - 2: TKFe (Equity)
    let assets = serde_json::json!({
        "assets": [
            [1, sudo_key, true, 1],
         [2, sudo_key, true, 1]
        ],
        "metadata": [
            [1, b"Reputation Token".to_vec(), b"TKFr".to_vec(), 0],
            [2, b"Equity Token".to_vec(), b"TKFe".to_vec(), 0]
        ],
        "accounts": [
            [1, sudo_key, 1_000_000_000_000_000_000u128], // 1e18
            [2, sudo_key, 80_000_000u128]
        ],
        "nextAssetId": 3
    });

    serde_json::json!({
        "sudo": { "key": sudo_key },
        "balances": { "balances": balances },
        "aura": { "authorities": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>() },
        "grandpa": { "authorities": initial_authorities.iter().map(|x| (x.1.clone(), 1)).collect::<Vec<_>>() },

        // Frontier/evm
        "evmChainId": { "chainId": chain_id },
        "evm": { "accounts": evm_accounts },
        "ethereum": {},          // default
        "baseFee": {},           // default
        "transactionPayment": {},// default

        // Manual seal (if runtime exposes it)
        "manualSeal": { "enable": enable_manual_seal },

        // Tokfin assets
        "tokfinAssets": assets
    })
}
