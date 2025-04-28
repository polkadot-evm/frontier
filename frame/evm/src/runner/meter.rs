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

use alloc::collections::btree_set::BTreeSet;
use evm::{
	gasometer::{GasCost, StorageTarget},
	Opcode,
};
use fp_evm::ACCOUNT_STORAGE_PROOF_SIZE;
use sp_core::{H160, H256};

/// An error that is returned when the storage limit has been exceeded.
#[derive(Debug, PartialEq)]
pub enum MeterError {
	LimitExceeded,
}

/// A meter for tracking the storage growth.
#[derive(Clone)]
pub struct StorageMeter {
	usage: u64,
	limit: u64,
	recorded_new_entries: BTreeSet<(H160, H256)>,
}

impl StorageMeter {
	/// Creates a new storage meter with the given limit.
	pub fn new(limit: u64) -> Self {
		Self {
			usage: 0,
			limit,
			recorded_new_entries: BTreeSet::new(),
		}
	}

	/// Records the given amount of storage usage. The amount is added to the current usage.
	/// If the limit is reached, an error is returned.
	pub fn record(&mut self, amount: u64) -> Result<(), MeterError> {
		let usage = self.usage.checked_add(amount).ok_or_else(|| {
			fp_evm::set_storage_oog();
			MeterError::LimitExceeded
		})?;

		if usage > self.limit {
			fp_evm::set_storage_oog();
			return Err(MeterError::LimitExceeded);
		}
		self.usage = usage;
		Ok(())
	}

	/// Records the storage growth for the given Opcode.
	pub fn record_dynamic_opcode_cost(
		&mut self,
		_opcode: Opcode,
		gas_cost: GasCost,
		target: StorageTarget,
	) -> Result<(), MeterError> {
		if let GasCost::SStore { original, new, .. } = gas_cost {
			// Validate if storage growth for the current slot has been accounted for within this transaction.
			// Comparing Original and new to determine if a new entry is being created is not sufficient, because
			// 'original' updates only at the end of the transaction. So, if a new entry
			// is created and updated multiple times within the same transaction, the storage growth is
			// accounted for multiple times, because 'original' is always zero for the subsequent updates.
			// To avoid this, we keep track of the new entries that are created within the transaction.
			let (address, index) = match target {
				StorageTarget::Slot(address, index) => (address, index),
				_ => return Ok(()),
			};
			let recorded = self.recorded_new_entries.contains(&(address, index));
			if !recorded && original == H256::default() && !new.is_zero() {
				self.record(ACCOUNT_STORAGE_PROOF_SIZE)?;
				self.recorded_new_entries.insert((address, index));
			}
		}
		Ok(())
	}

	/// Returns the current usage of storage.
	pub fn usage(&self) -> u64 {
		self.usage
	}

	/// Returns the limit of storage.
	pub fn limit(&self) -> u64 {
		self.limit
	}

	/// Returns the amount of storage that is available before the limit is reached.
	pub fn available(&self) -> u64 {
		self.limit.saturating_sub(self.usage)
	}

	/// Map storage usage to the gas cost.
	pub fn storage_to_gas(&self, ratio: u64) -> u64 {
		self.usage.saturating_mul(ratio)
	}
}
#[cfg(test)]
mod test {
	use super::*;

	/// Tests the basic functionality of StorageMeter.
	#[test]
	fn test_basic_functionality() {
		let limit = 100;
		let mut meter = StorageMeter::new(limit);

		assert_eq!(meter.usage(), 0);
		assert_eq!(meter.limit(), limit);

		let amount = 10;
		meter.record(amount).unwrap();
		assert_eq!(meter.usage(), amount);
	}

	/// Tests the behavior of StorageMeter when reaching the limit.
	#[test]
	fn test_reaching_limit() {
		let limit = 100;
		let mut meter = StorageMeter::new(limit);

		// Approaching the limit without exceeding
		meter.record(limit - 1).unwrap();
		assert_eq!(meter.usage(), limit - 1);

		// Reaching the limit exactly
		meter.record(1).unwrap();
		assert_eq!(meter.usage(), limit);

		// Exceeding the limit
		let res = meter.record(1);
		assert_eq!(meter.usage(), limit);
		assert!(res.is_err());
		assert_eq!(res, Err(MeterError::LimitExceeded));
	}

	/// Tests the record of dynamic opcode cost.
	#[test]
	fn test_record_dynamic_opcode_cost() {
		let limit = 200;
		let mut meter = StorageMeter::new(limit);

		// Existing storage entry is updated. No change in storage growth.
		let gas_cost = GasCost::SStore {
			original: H256::from_low_u64_be(1),
			current: Default::default(),
			new: H256::from_low_u64_be(2),
			target_is_cold: false,
		};
		let target = StorageTarget::Slot(H160::default(), H256::from_low_u64_be(1));

		meter
			.record_dynamic_opcode_cost(Opcode::SSTORE, gas_cost, target)
			.unwrap();
		assert_eq!(meter.usage(), 0);

		// New storage entry is created. Storage growth is recorded.
		let gas_cost = GasCost::SStore {
			original: H256::default(),
			current: Default::default(),
			new: H256::from_low_u64_be(1),
			target_is_cold: false,
		};
		meter
			.record_dynamic_opcode_cost(Opcode::SSTORE, gas_cost, target)
			.unwrap();
		assert_eq!(meter.usage(), ACCOUNT_STORAGE_PROOF_SIZE);

		// Try to record the same storage growth again. No change in storage growth.
		let gas_cost = GasCost::SStore {
			original: H256::default(),
			current: Default::default(),
			new: H256::from_low_u64_be(1),
			target_is_cold: false,
		};
		meter
			.record_dynamic_opcode_cost(Opcode::SSTORE, gas_cost, target)
			.unwrap();
		assert_eq!(meter.usage(), ACCOUNT_STORAGE_PROOF_SIZE);

		// New storage entry is created. Storage growth is recorded. The limit is reached.
		let gas_cost = GasCost::SStore {
			original: H256::default(),
			current: Default::default(),
			new: H256::from_low_u64_be(2),
			target_is_cold: false,
		};
		let target = StorageTarget::Slot(H160::default(), H256::from_low_u64_be(2));

		let res = meter.record_dynamic_opcode_cost(Opcode::SSTORE, gas_cost, target);
		assert!(res.is_err());
		assert_eq!(res, Err(MeterError::LimitExceeded));
		assert_eq!(meter.usage(), 116);
	}
}
