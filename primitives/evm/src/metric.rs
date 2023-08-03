use evm::{gasometer::GasCost, Opcode};
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_runtime::{traits::CheckedAdd, Saturating};

#[derive(Debug, PartialEq)]
/// Metric error.
pub enum MetricError {
	/// The metric usage exceeds the limit.
	LimitExceeded,
	/// Invalid Base Cost.
	InvalidBaseCost,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// A struct that keeps track of metric usage and limit.
pub struct Metric<T> {
	limit: T,
	usage: T,
}

impl<T> Metric<T>
where
	T: CheckedAdd + Saturating + PartialOrd + Copy,
{
	/// Creates a new `Metric` instance with the given base cost and limit.
	///
	/// # Errors
	///
	/// Returns `MetricError::InvalidBaseCost` if the base cost is greater than the limit.
	pub fn new(base_cost: T, limit: T) -> Result<Self, MetricError> {
		if base_cost > limit {
			return Err(MetricError::InvalidBaseCost);
		}
		Ok(Self {
			limit,
			usage: base_cost,
		})
	}

	/// Records the cost of an operation and updates the usage.
	///
	/// # Errors
	///
	/// Returns `MetricError::LimitExceeded` if the metric usage exceeds the limit.
	fn record_cost(&mut self, cost: T) -> Result<(), MetricError> {
		let usage = self
			.usage
			.checked_add(&cost)
			.ok_or(MetricError::LimitExceeded)?;

		if usage > self.limit {
			return Err(MetricError::LimitExceeded);
		}
		self.usage = usage;
		Ok(())
	}

	/// Refunds the given amount.
	fn refund(&mut self, amount: T) {
		self.usage = self.usage.saturating_sub(amount);
	}

	/// Returns the usage.
	fn usage(&self) -> T {
		self.usage
	}
}

/// A struct that keeps track of the proof size and limit.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProofSizeMeter(Metric<u64>);

impl ProofSizeMeter {
	/// `System::Account` 16(hash) + 20 (key) + 60 (AccountInfo::max_encoded_len)
	pub const ACCOUNT_BASIC_PROOF_SIZE: u64 = 96;
	/// `AccountCodesMetadata` read, temptatively 16 (hash) + 20 (key) + 40 (CodeMetadata).
	pub const ACCOUNT_CODES_METADATA_PROOF_SIZE: u64 = 76;
	/// Account basic proof size + 5 bytes max of `decode_len` call.
	pub const IS_EMPTY_CHECK_PROOF_SIZE: u64 = 93;

	/// Creates a new `ProofSizeMetric` instance with the given limit.
	pub fn new(base_cost: u64, limit: u64) -> Result<Self, MetricError> {
		Ok(Self(Metric::new(base_cost, limit)?))
	}

	/// Records the size of the proof and updates the usage.
	///
	/// # Errors
	///
	/// Returns `MetricError::LimitExceeded` if the proof size exceeds the limit.
	pub fn record_proof_size(&mut self, size: u64) -> Result<(), MetricError> {
		self.0.record_cost(size)
	}

	/// Refunds the given amount of proof size.
	pub fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}

	/// Returns the proof size usage.
	pub fn usage(&self) -> u64 {
		self.0.usage()
	}

	/// Returns the proof size limit.
	pub fn limit(&self) -> u64 {
		self.0.limit
	}
}

/// A struct that keeps track of the ref_time usage and limit.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RefTimeMeter(Metric<u64>);

impl RefTimeMeter {
	/// Creates a new `RefTimeMetric` instance with the given limit.
	pub fn new(limit: u64) -> Result<Self, MetricError> {
		Ok(Self(Metric::new(0, limit)?))
	}

	/// Records the ref_time and updates the usage.
	pub fn record_ref_time(&mut self, ref_time: u64) -> Result<(), MetricError> {
		self.0.record_cost(ref_time)
	}

	/// Refunds the given amount of ref_time.
	pub fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}
}
/// A struct that keeps track of storage usage (newly created storage) and limit.
pub struct StorageMeter(Metric<u64>);

impl StorageMeter {
	/// Creates a new `StorageMetric` instance with the given limit.
	pub fn new(limit: u64) -> Result<Self, MetricError> {
		Ok(Self(Metric::new(0, limit)?))
	}

	/// Refunds the given amount of storage.
	fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}

	/// Records the dynamic opcode cost and updates the storage usage.
	///
	/// # Errors
	///
	/// Returns `MetricError::LimitExceeded` if the storage usage exceeds the storage limit.
	fn record_dynamic_opcode_cost(
		&mut self,
		_opcode: Opcode,
		gas_cost: GasCost,
	) -> Result<(), MetricError> {
		let cost = match gas_cost {
			GasCost::Create => {
				// TODO record cost for create
				0
			}
			GasCost::Create2 { len } => {
				// len in bytes ??
				len.try_into().map_err(|_| MetricError::LimitExceeded)?
			}
			GasCost::SStore { .. } => {
				// TODO record cost for sstore
				0
			}
			_ => return Ok(()),
		};
		self.0.record_cost(cost)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_init() {
		let metric = Metric::<u64>::new(0, 100).unwrap();
		assert_eq!(metric.limit, 100);
		assert_eq!(metric.usage, 0);

		// base cost > limit
		let metric = Metric::<u64>::new(100, 0).err();
		assert_eq!(metric, Some(MetricError::InvalidBaseCost));
	}

	#[test]
	fn test_record_cost() {
		let mut metric = Metric::<u64>::new(0, 100).unwrap();
		assert_eq!(metric.record_cost(10), Ok(()));
		assert_eq!(metric.usage, 10);
		assert_eq!(metric.record_cost(90), Ok(()));
		assert_eq!(metric.usage, 100);

		// exceed limit
		assert_eq!(metric.record_cost(1), Err(MetricError::LimitExceeded));
		assert_eq!(metric.usage, 100);
	}

	#[test]
	fn test_refund() {
		let mut metric = Metric::<u64>::new(0, 100).unwrap();
		assert_eq!(metric.record_cost(10), Ok(()));
		assert_eq!(metric.usage, 10);
		metric.refund(10);
		assert_eq!(metric.usage, 0);

		// refund more than usage
		metric.refund(10);
		assert_eq!(metric.usage, 0);
	}

	#[test]
	fn test_storage_metric() {
		let mut metric = StorageMeter::new(100).unwrap();
		assert_eq!(metric.0.usage, 0);
		assert_eq!(metric.0.limit, 100);
		assert_eq!(metric.0.record_cost(10), Ok(()));
		assert_eq!(metric.0.usage, 10);
		assert_eq!(metric.0.record_cost(90), Ok(()));
		assert_eq!(metric.0.usage, 100);
		assert_eq!(metric.0.record_cost(1), Err(MetricError::LimitExceeded));
		assert_eq!(metric.0.usage, 100);
		metric.0.refund(10);
		assert_eq!(metric.0.usage, 90);
		metric.refund(10);
		assert_eq!(metric.0.usage, 80);
	}
}
