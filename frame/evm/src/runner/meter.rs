use evm::{gasometer::GasCost, Opcode};
use fp_evm::ACCOUNT_STORAGE_PROOF_SIZE;
use sp_core::H256;

/// A meter for tracking the storage growth.
#[derive(Clone, Copy)]
pub struct StorageMeter {
	usage: u64,
	limit: u64,
}

/// An error that is returned when the storage limit has been exceeded.
#[derive(Debug, PartialEq)]
pub enum MeterError {
	LimitExceeded,
}

impl StorageMeter {
	/// Creates a new storage meter with the given limit.
	pub fn new(limit: u64) -> Self {
		Self { usage: 0, limit }
	}

	/// Records the given amount of storage usage. The amount is added to the current usage.
	/// If the limit is reached, an error is returned.
	pub fn record(&mut self, amount: u64) -> Result<(), MeterError> {
		self.usage = self.usage.checked_add(amount).ok_or_else(|| {
			self.usage = self.limit;
			MeterError::LimitExceeded
		})?;

		if self.usage > self.limit {
			return Err(MeterError::LimitExceeded);
		}
		Ok(())
	}

	/// Records the storage growth for the given Opcode.
	pub fn record_dynamic_opcode_cost(
		&mut self,
		_opcode: Opcode,
		gas_cost: GasCost,
	) -> Result<(), MeterError> {
		match gas_cost {
			GasCost::SStore { original, new, .. }
				if original == H256::default() && !new.is_zero() =>
			{
				self.record(ACCOUNT_STORAGE_PROOF_SIZE)
			}
			_ => Ok(()),
		}
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
		assert_eq!(meter.usage(), limit + 1);
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
		meter
			.record_dynamic_opcode_cost(Opcode::SSTORE, gas_cost)
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
			.record_dynamic_opcode_cost(Opcode::SSTORE, gas_cost)
			.unwrap();
		assert_eq!(meter.usage(), ACCOUNT_STORAGE_PROOF_SIZE);

		// New storage entry is created. Storage growth is recorded. The limit is reached.
		let gas_cost = GasCost::SStore {
			original: H256::default(),
			current: Default::default(),
			new: H256::from_low_u64_be(2),
			target_is_cold: false,
		};
		let res = meter.record_dynamic_opcode_cost(Opcode::SSTORE, gas_cost);
		assert!(res.is_err());
		assert_eq!(res, Err(MeterError::LimitExceeded));
		assert_eq!(meter.usage(), 232);
	}
}
