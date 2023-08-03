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

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unused_crate_dependencies)]

mod metric;
mod precompile;
mod validation;

use frame_support::weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight};
use metric::{ProofSizeMeter, RefTimeMeter};
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_core::{H160, H256, U256};
use sp_runtime::Perbill;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub use evm::{
	backend::{Basic as Account, Log},
	Config, ExitReason, Opcode,
};

pub use self::{
	precompile::{
		Context, ExitError, ExitRevert, ExitSucceed, IsPrecompileResult, LinearCostPrecompile,
		Precompile, PrecompileFailure, PrecompileHandle, PrecompileOutput, PrecompileResult,
		PrecompileSet, Transfer,
	},
	validation::{
		CheckEvmTransaction, CheckEvmTransactionConfig, CheckEvmTransactionInput,
		InvalidEvmTransactionError,
	},
};

#[derive(Clone, Eq, PartialEq, Default, Debug, Encode, Decode)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// External input from the transaction.
pub struct Vicinity {
	/// Current transaction gas price.
	pub gas_price: U256,
	/// Origin of the transaction.
	pub origin: H160,
}

/// `System::Account` 16(hash) + 20 (key) + 60 (AccountInfo::max_encoded_len)
pub const ACCOUNT_BASIC_PROOF_SIZE: u64 = 96;
/// `AccountCodesMetadata` read, temptatively 16 (hash) + 20 (key) + 40 (CodeMetadata).
pub const ACCOUNT_CODES_METADATA_PROOF_SIZE: u64 = 76;
/// 16 (hash1) + 20 (key1) + 16 (hash2) + 32 (key2) + 32 (value)
pub const ACCOUNT_STORAGE_PROOF_SIZE: u64 = 116;
/// Fixed trie 32 byte hash.
pub const WRITE_PROOF_SIZE: u64 = 32;
/// Account basic proof size + 5 bytes max of `decode_len` call.
pub const IS_EMPTY_CHECK_PROOF_SIZE: u64 = 93;

pub enum AccessedStorage {
	AccountCodes(H160),
	AccountStorages((H160, H256)),
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct WeightInfo {
	pub ref_time_meter: Option<RefTimeMeter>,
	pub proof_size_meter: Option<ProofSizeMeter>,
}

impl WeightInfo {
	pub fn new_from_weight_limit(
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
	) -> Result<Option<Self>, &'static str> {
		Ok(match (weight_limit, proof_size_base_cost) {
			(None, _) => None,
			(Some(weight_limit), Some(proof_size_base_cost))
				if weight_limit.proof_size() >= proof_size_base_cost =>
			{
				Some(WeightInfo {
					ref_time_meter: Some(
						RefTimeMeter::new(weight_limit.ref_time())
							.map_err(|_| "invalid ref time base cost")?,
					),
					proof_size_meter: Some(
						ProofSizeMeter::new(proof_size_base_cost, weight_limit.proof_size())
							.map_err(|_| "invalid proof size base cost")?,
					),
				})
			}
			(Some(weight_limit), None) => Some(WeightInfo {
				ref_time_meter: Some(
					RefTimeMeter::new(weight_limit.ref_time())
						.map_err(|_| "invalid ref time base cost")?,
				),
				proof_size_meter: None,
			}),
			_ => return Err("must provide Some valid weight limit or None"),
		})
	}

	pub fn try_record_ref_time_or_fail(&mut self, cost: u64) -> Result<(), ExitError> {
		if let Some(ref_time_meter) = self.ref_time_meter.as_mut() {
			ref_time_meter
				.record_ref_time(cost)
				.map_err(|_| ExitError::OutOfGas)?;
		}

		Ok(())
	}
	pub fn try_record_proof_size_or_fail(&mut self, cost: u64) -> Result<(), ExitError> {
		if let Some(proof_size_meter) = self.proof_size_meter.as_mut() {
			proof_size_meter
				.record_proof_size(cost)
				.map_err(|_| ExitError::OutOfGas)?;
		}

		Ok(())
	}

	pub fn refund_proof_size(&mut self, amount: u64) {
		self.proof_size_meter.as_mut().map(|proof_size_meter| {
			proof_size_meter.refund(amount);
		});
	}

	pub fn proof_size_usage(&self) -> u64 {
		self.proof_size_meter
			.map_or(0, |proof_size_meter| proof_size_meter.usage())
	}

	pub fn refund_ref_time(&mut self, amount: u64) {
		self.ref_time_meter.as_mut().map(|ref_time_meter| {
			ref_time_meter.refund(amount);
		});
	}
}

#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct UsedGas {
	/// The used_gas as returned by the evm gasometer on exit.
	pub standard: U256,
	/// The result of applying a gas ratio to the most used
	/// external metric during the evm execution.
	pub effective: U256,
}

#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExecutionInfoV2<T> {
	pub exit_reason: ExitReason,
	pub value: T,
	pub used_gas: UsedGas,
	pub weight_info: Option<WeightInfo>,
	pub logs: Vec<Log>,
}

pub type CallInfo = ExecutionInfoV2<Vec<u8>>;
pub type CreateInfo = ExecutionInfoV2<H160>;

#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CallOrCreateInfo {
	Call(CallInfo),
	Create(CreateInfo),
}

#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExecutionInfo<T> {
	pub exit_reason: ExitReason,
	pub value: T,
	pub used_gas: U256,
	pub logs: Vec<Log>,
}

/// Account definition used for genesis block construction.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GenesisAccount {
	/// Account nonce.
	pub nonce: U256,
	/// Account balance.
	pub balance: U256,
	/// Full account storage.
	pub storage: BTreeMap<H256, H256>,
	/// Account code.
	pub code: Vec<u8>,
}

/// Trait that outputs the current transaction gas price.
pub trait FeeCalculator {
	/// Return the minimal required gas price.
	fn min_gas_price() -> (U256, Weight);
}

impl FeeCalculator for () {
	fn min_gas_price() -> (U256, Weight) {
		(U256::zero(), Weight::zero())
	}
}

/// `WeightPerGas` is an approximate ratio of the amount of Weight per Gas.
/// u64 works for approximations because Weight is a very small unit compared to gas.
///
/// `GAS_PER_MILLIS * WEIGHT_MILLIS_PER_BLOCK * TXN_RATIO ~= BLOCK_GAS_LIMIT`
/// `WEIGHT_PER_GAS = WEIGHT_REF_TIME_PER_MILLIS / GAS_PER_MILLIS
///                 = WEIGHT_REF_TIME_PER_MILLIS / (BLOCK_GAS_LIMIT / TXN_RATIO / WEIGHT_MILLIS_PER_BLOCK)
///                 = TXN_RATIO * (WEIGHT_REF_TIME_PER_MILLIS * WEIGHT_MILLIS_PER_BLOCK) / BLOCK_GAS_LIMIT`
///
/// For example, given the 2000ms Weight, from which 75% only are used for transactions,
/// the total EVM execution gas limit is `GAS_PER_MILLIS * 2000 * 75% = BLOCK_GAS_LIMIT`.
pub fn weight_per_gas(
	block_gas_limit: u64,
	txn_ratio: Perbill,
	weight_millis_per_block: u64,
) -> u64 {
	let weight_per_block = WEIGHT_REF_TIME_PER_MILLIS.saturating_mul(weight_millis_per_block);
	let weight_per_gas = (txn_ratio * weight_per_block).saturating_div(block_gas_limit);
	assert!(
		weight_per_gas >= 1,
		"WeightPerGas must greater than or equal with 1"
	);
	weight_per_gas
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_weight_per_gas() {
		assert_eq!(
			weight_per_gas(15_000_000, Perbill::from_percent(75), 500),
			25_000
		);
		assert_eq!(
			weight_per_gas(75_000_000, Perbill::from_percent(75), 2_000),
			20_000
		);
		assert_eq!(
			weight_per_gas(1_500_000_000_000, Perbill::from_percent(75), 2_000),
			1
		);
	}
}
