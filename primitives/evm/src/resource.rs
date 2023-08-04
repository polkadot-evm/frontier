use evm::{gasometer::GasCost, Opcode};
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_runtime::{traits::CheckedAdd, Saturating};

#[derive(Debug, PartialEq)]
/// Resource error.
pub enum ResourceError {
	/// The Resource usage exceeds the limit.
	LimitExceeded,
	/// Invalid Base Cost.
	InvalidBaseCost,
}

/// A struct that keeps track of resource usage and limit.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Resource<T> {
	limit: T,
	usage: T,
}

impl<T> Resource<T>
where
	T: CheckedAdd + Saturating + PartialOrd + Copy,
{
	/// Creates a new `Resource` instance with the given base cost and limit.
	///
	/// # Errors
	///
	/// Returns `ResourceError::InvalidBaseCost` if the base cost is greater than the limit.
	pub fn new(base_cost: T, limit: T) -> Result<Self, ResourceError> {
		if base_cost > limit {
			return Err(ResourceError::InvalidBaseCost);
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
	/// Returns `ResourceError::LimitExceeded` if the Resource usage exceeds the limit.
	fn record_cost(&mut self, cost: T) -> Result<(), ResourceError> {
		let usage = self
			.usage
			.checked_add(&cost)
			.ok_or(ResourceError::LimitExceeded)?;

		if usage > self.limit {
			return Err(ResourceError::LimitExceeded);
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
pub struct ProofSizeMeter(Resource<u64>);

impl ProofSizeMeter {
	/// Creates a new `ProofSizeResource` instance with the given limit.
	pub fn new(base_cost: u64, limit: u64) -> Result<Self, ResourceError> {
		Ok(Self(Resource::new(base_cost, limit)?))
	}

	/// Records the size of the proof and updates the usage.
	///
	/// # Errors
	///
	/// Returns `ResourceError::LimitExceeded` if the proof size exceeds the limit.
	pub fn record_proof_size(&mut self, size: u64) -> Result<(), ResourceError> {
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
pub struct RefTimeMeter(Resource<u64>);

impl RefTimeMeter {
	/// Creates a new `RefTimeResource` instance with the given limit.
	pub fn new(limit: u64) -> Result<Self, ResourceError> {
		Ok(Self(Resource::new(0, limit)?))
	}

	/// Records the ref_time and updates the usage.
	pub fn record_ref_time(&mut self, ref_time: u64) -> Result<(), ResourceError> {
		self.0.record_cost(ref_time)
	}

	/// Refunds the given amount of ref_time.
	pub fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}
}
/// A struct that keeps track of storage usage (newly created storage) and limit.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, TypeInfo)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StorageMeter(Resource<u64>);

impl StorageMeter {
	/// Creates a new `StorageResource` instance with the given limit.
	pub fn new(limit: u64) -> Result<Self, ResourceError> {
		Ok(Self(Resource::new(0, limit)?))
	}

	/// Refunds the given amount of storage.
	fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}

	/// Records the dynamic opcode cost and updates the storage usage.
	///
	/// # Errors
	///
	/// Returns `ResourceError::LimitExceeded` if the storage usage exceeds the storage limit.
	fn record_dynamic_opcode_cost(
		&mut self,
		_opcode: Opcode,
		gas_cost: GasCost,
	) -> Result<(), ResourceError> {
		let cost = match gas_cost {
			GasCost::Create => {
				// TODO record cost for create
				0
			}
			GasCost::Create2 { len } => {
				// len in bytes ??
				len.try_into().map_err(|_| ResourceError::LimitExceeded)?
			}
			GasCost::SStore { .. } => {
				// TODO record cost for sstore
				0
			}
			_ => return Ok(()),
		};
		self.0.record_cost(cost)
	}

	fn record_external_operation(&mut self, operation: evm::ExternalOperation) {
		match operation {
			evm::ExternalOperation::Write => {
				// Todo record cost for write
			}
			_ => {}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_init() {
		let resource = Resource::<u64>::new(0, 100).unwrap();
		assert_eq!(resource.limit, 100);
		assert_eq!(resource.usage, 0);

		// base cost > limit
		let resource = Resource::<u64>::new(100, 0).err();
		assert_eq!(resource, Some(ResourceError::InvalidBaseCost));
	}

	#[test]
	fn test_record_cost() {
		let mut resource = Resource::<u64>::new(0, 100).unwrap();
		assert_eq!(resource.record_cost(10), Ok(()));
		assert_eq!(resource.usage, 10);
		assert_eq!(resource.record_cost(90), Ok(()));
		assert_eq!(resource.usage, 100);

		// exceed limit
		assert_eq!(resource.record_cost(1), Err(ResourceError::LimitExceeded));
		assert_eq!(resource.usage, 100);
	}

	#[test]
	fn test_refund() {
		let mut resource = Resource::<u64>::new(0, 100).unwrap();
		assert_eq!(resource.record_cost(10), Ok(()));
		assert_eq!(resource.usage, 10);
		resource.refund(10);
		assert_eq!(resource.usage, 0);

		// refund more than usage
		resource.refund(10);
		assert_eq!(resource.usage, 0);
	}

	#[test]
	fn test_storage_resource() {
		let mut resource = StorageMeter::new(100).unwrap();
		assert_eq!(resource.0.usage, 0);
		assert_eq!(resource.0.limit, 100);
		assert_eq!(resource.0.record_cost(10), Ok(()));
		assert_eq!(resource.0.usage, 10);
		assert_eq!(resource.0.record_cost(90), Ok(()));
		assert_eq!(resource.0.usage, 100);
		assert_eq!(resource.0.record_cost(1), Err(ResourceError::LimitExceeded));
		assert_eq!(resource.0.usage, 100);
		resource.0.refund(10);
		assert_eq!(resource.0.usage, 90);
		resource.refund(10);
		assert_eq!(resource.0.usage, 80);
	}
}
