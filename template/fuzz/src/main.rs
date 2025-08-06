// Copyright 2025 Security Research Labs GmbH
//
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
// limitations under the License./ DEALINGS IN THE SOFTWARE.
mod grammar;

use frame_system::Account;
use fuzzed_runtime::{
	Balance, Balances, BalancesConfig, Runtime,
};
use grammar::FuzzData;
use pallet_balances::{Holds, TotalIssuance};
use pallet_evm::{GasWeightMapping, Runner};
use sp_core::H160;
use sp_runtime::{
	traits::Header,
	BuildStorage,
};
use sp_state_machine::BasicExternalities;

fn main() {
	ziggy::fuzz!(|data: FuzzData| {
		let config = evm::Config::cancun();
		let source = H160::default();
		let target = H160::from_low_u64_ne(02);
		let gas_limit: u64 = 1_000_000;
		new_test_ext().execute_with(|| {
			let initial_total_issuance = TotalIssuance::<Runtime>::get();
			let mut weight_limit =
				pallet_evm::FixedGasWeightMapping::<Runtime>::gas_to_weight(gas_limit, true);
			if data.check_proof_size {
				*weight_limit.proof_size_mut() = weight_limit.proof_size() / 2;
			}
			let max_proof_size = weight_limit.proof_size();
			let mut contract = vec![];
			for op in &data.contract {
				op.to_bytes(&mut contract);
			}
			pallet_evm::AccountCodes::<Runtime>::insert(target, contract);
			#[cfg(not(feature = "fuzzing"))]
			let now = std::time::Instant::now();
			let res = <Runtime as pallet_evm::Config>::Runner::call(
				H160::default(),
				target,
				data.call_data,
				data.value.into(),
				gas_limit,
				Some(1000_000_000.into()),
				None,
				None,
				Vec::new(),
				true,
				true,
				Some(weight_limit),
				Some(0),
				&<Runtime as pallet_evm::Config>::config().clone(),
			);
			let proof_size = match res {
				Ok(ref info) => info
					.weight_info
					.expect("weight info")
					.proof_size_usage
					.expect("proof size usage"),
				Err(ref info) => 0,
			};
			assert!(proof_size <= max_proof_size);
			check_invariants(initial_total_issuance);
		});
	});
}

pub fn new_test_ext() -> BasicExternalities {
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_runtime::{app_crypto::ByteArray, BuildStorage};
	let accounts: Vec<fuzzed_runtime::AccountId> = (0..5).map(|i| [i; 32].into()).collect();
	let t = fuzzed_runtime::RuntimeGenesisConfig {
		system: Default::default(),
		balances: BalancesConfig {
			// Configure endowed accounts with initial balance of 1 << 80.
			balances: accounts.iter().cloned().map(|k| (k, 1 << 80)).collect(),
			..Default::default()
		},
		base_fee: Default::default(),
		evm_chain_id: Default::default(),
		aura: fuzzed_runtime::AuraConfig {
			authorities: vec![AuraId::from_slice(&[0; 32]).unwrap()],
		},
		sudo: fuzzed_runtime::SudoConfig { key: None },
		transaction_payment: Default::default(),
		grandpa: Default::default(),
		manual_seal: Default::default(),
		ethereum: Default::default(),
		evm: Default::default(),
	}
	.build_storage()
	.unwrap();
	BasicExternalities::new(t)
}

fn check_invariants(initial_total_issuance: Balance) {
	// After execution of all blocks, we run general polkadot-sdk invariants
	let mut counted_free: Balance = 0;
	let mut counted_reserved: Balance = 0;
	for (account, info) in Account::<Runtime>::iter() {
		let consumers = info.consumers;
		let providers = info.providers;
		assert!(!(consumers > 0 && providers == 0), "Invalid c/p state");
		counted_free += info.data.free;
		counted_reserved += info.data.reserved;
		let max_lock: Balance = Balances::locks(&account)
			.iter()
			.map(|l| l.amount)
			.max()
			.unwrap_or_default();
		assert_eq!(
			max_lock, info.data.frozen,
			"Max lock should be equal to frozen balance"
		);
		let sum_holds: Balance = Holds::<Runtime>::get(account)
			.iter()
			.map(|l| l.amount)
			.sum();
		assert!(
			sum_holds <= info.data.reserved,
			"Sum of all holds ({sum_holds}) should be less than or equal to reserved balance {}",
			info.data.reserved
		);
	}
	let total_issuance = TotalIssuance::<Runtime>::get();
	let counted_issuance = counted_free + counted_reserved;
	assert_eq!(total_issuance, counted_issuance);
	assert!(total_issuance <= initial_total_issuance);
}
