use evm::ExitError;
use sp_runtime::{
	traits::{CheckedAdd, Zero},
	Saturating,
};

/// A trait for metering different foreign metrics.
pub trait Metric<T> {
	fn try_consume(&self, cost: T) -> Result<T, ExitError>;
	fn refund(&mut self, amount: T);
	fn record(&mut self, cost: T) -> Result<(), ExitError>;
}

pub struct BasicMetric<T> {
	limit: T,
	usage: T,
}

impl<T> BasicMetric<T>
where
	T: Zero,
{
	pub fn new(limit: T) -> Self {
		Self {
			limit,
			usage: Zero::zero(),
		}
	}
}

impl<T> Metric<T> for BasicMetric<T>
where
	T: CheckedAdd + Saturating + PartialOrd + Copy,
{
	fn try_consume(&self, cost: T) -> Result<T, ExitError> {
		let usage = self.usage.checked_add(&cost).ok_or(ExitError::OutOfGas)?;
		if usage > self.limit {
			return Err(ExitError::OutOfGas);
		}
		Ok(usage)
	}

	fn refund(&mut self, amount: T) {
		self.usage = self.usage.saturating_sub(amount);
	}

    fn record(&mut self, cost: T) -> Result<(), ExitError> {

        Ok(())
    }
}

pub struct StorageMetric(pub BasicMetric<u64>);

impl Metric<u64> for StorageMetric {
	fn try_consume(&self, cost: u64) -> Result<u64, ExitError> {
		self.0.try_consume(cost)
	}

	fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}

	fn record(&mut self, cost: u64) -> Result<(), ExitError> {
		Ok(())
	}
}
