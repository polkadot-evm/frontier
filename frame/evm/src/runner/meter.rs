/// The size of a storage key and storage value in bytes.
pub const STORAGE_SIZE: u64 = 64;

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
	/// The usage will saturate at `u64::MAX`.
	pub fn record(&mut self, amount: u64) {
		self.usage = self.usage.saturating_add(amount);
	}

	/// Returns the current usage of storage.
	pub fn usage(&self) -> u64 {
		self.usage
	}

	/// Returns the amount of storage that is available before the limit is reached.
	pub fn available(&self) -> u64 {
		self.limit.saturating_sub(self.usage)
	}

	/// Merge the given storage meter into the current one.
	pub fn merge(&mut self, other: Option<Self>) {
		self.usage = self
			.usage
			.saturating_add(other.map_or(0, |meter| meter.usage));
	}

	/// Map storage usage to the gas cost.
	pub fn storage_to_gas(&self, ratio: u64) -> u64 {
		self.usage.saturating_mul(ratio)
	}

	/// Checks if the current usage of storage is within the limit.
	///
	/// # Errors
	///
	/// Returns `MeterError::ResourceLimitExceeded` if the limit has been exceeded.
	pub fn check_limit(&self) -> Result<(), MeterError> {
		if self.usage > self.limit {
			Err(MeterError::LimitExceeded)
		} else {
			Ok(())
		}
	}
}

// Generate a comprehensive unit test suite for the StorageMeter:
// - Make sure to cover all the edge cases.
// - Group the tests into a module so that they are not included in the crate documentation.
// - Group similar or related tests in seperate functions.
// - Document the different tests explaining the use case
#[cfg(test)]
mod test {
	use super::*;

	#[cfg(test)]
	mod tests {
		use super::*;

		/// Tests the basic functionality of StorageMeter.
		#[test]
		fn test_basic_functionality() {
			let limit = 100;
			let mut meter = StorageMeter::new(limit);

			assert_eq!(meter.usage(), 0);
			assert_eq!(meter.available(), limit);

			let amount = 10;
			meter.record(amount);
			assert_eq!(meter.usage(), amount);
			assert_eq!(meter.available(), limit - amount);

			assert!(meter.check_limit().is_ok());
		}

		/// Tests the behavior of StorageMeter when reaching the limit.
		#[test]
		fn test_reaching_limit() {
			let limit = 100;
			let mut meter = StorageMeter::new(limit);

			// Approaching the limit without exceeding
			meter.record(limit - 1);
			assert_eq!(meter.usage(), limit - 1);
			assert!(meter.check_limit().is_ok());

			// Reaching the limit exactly
			meter.record(1);
			assert_eq!(meter.usage(), limit);
			assert!(meter.check_limit().is_ok());

			// Exceeding the limit
			meter.record(1);
			assert_eq!(meter.usage(), limit + 1);
			assert_eq!(meter.check_limit(), Err(MeterError::LimitExceeded));
		}

		/// Tests the behavior of StorageMeter with saturation.
		#[test]
		fn test_saturation_behavior() {
			let limit = u64::MAX;
			let mut meter = StorageMeter::new(limit);

			// Reaching the limit
			meter.record(limit);
			assert_eq!(meter.usage(), limit);
			assert_eq!(meter.available(), 0);
			assert!(meter.check_limit().is_ok());

			// Exceeding the limit using saturating_add
			meter.record(1);
			assert_eq!(meter.usage(), limit);
			assert_eq!(meter.available(), 0);
			assert!(meter.check_limit().is_ok());
		}
	}
}
