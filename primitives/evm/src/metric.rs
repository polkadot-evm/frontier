use evm::{gasometer::GasCost, Opcode};
use sp_runtime::{
	traits::{CheckedAdd, Zero},
	Saturating,
};

#[derive(Debug, PartialEq)]
/// Metric error.
enum MetricError {
	/// The metric usage exceeds the limit.
	LimitExceeded,
}

/// A struct that keeps track of metric usage and limit.
pub struct Metric<T> {
	limit: T,
	usage: T,
}

impl<T> Metric<T>
where
	T: Zero,
{
	/// Creates a new `Metric` instance with the given limit.
	pub fn new(limit: T) -> Self {
		Self {
			limit,
			usage: Zero::zero(),
		}
	}
}

impl<T> Metric<T>
where
	T: CheckedAdd + Saturating + PartialOrd + Copy,
{
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
}

/// A struct that keeps track of new storage usage and limit.
pub struct StorageMetric(Metric<u64>);

impl StorageMetric {
	/// Creates a new `StorageMetric` instance with the given limit.
	pub fn new(limit: u64) -> Self {
		Self(Metric::new(limit))
	}

	/// Refunds the given amount of storage gas.
	fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}

	/// Records the dynamic opcode cost and updates the storage usage.
	///
	/// # Errors
	///
	/// Returns `MetricError::LimitExceeded` if the storage gas usage exceeds the storage gas limit.
	fn record_dynamic_opcode_cost(
		&mut self,
		_opcode: Opcode,
		gas_cost: GasCost,
	) -> Result<(), MetricError> {
		let cost = match gas_cost {
			GasCost::Create => 0,
			GasCost::Create2 { len } => len.try_into().map_err(|_| MetricError::LimitExceeded)?,
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
		let metric = Metric::<u64>::new(100);
		assert_eq!(metric.limit, 100);
		assert_eq!(metric.usage, 0);
	}

	#[test]
	fn test_try_consume() {
		let mut metric = Metric::<u64>::new(100);
		assert_eq!(metric.record_cost(10), Ok(()));
		assert_eq!(metric.usage, 10);
		assert_eq!(metric.record_cost(90), Ok(()));
		assert_eq!(metric.usage, 100);
		assert_eq!(metric.record_cost(1), Err(MetricError::LimitExceeded));
		assert_eq!(metric.usage, 100);
	}

	#[test]
	fn test_refund() {
		let mut metric = Metric::<u64>::new(100);
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
		let mut metric = StorageMetric::new(100);
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
