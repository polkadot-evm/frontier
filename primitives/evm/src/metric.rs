use evm::{gasometer::GasCost, ExitError, Opcode};
use sp_runtime::{
	traits::{CheckedAdd, Zero},
	Saturating,
};

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

impl<T> BasicMetric<T>
where
	T: CheckedAdd + Saturating + PartialOrd + Copy,
{
	fn try_consume(&mut self, cost: T) -> Result<(), ExitError> {
		let usage = self.usage.checked_add(&cost).ok_or(ExitError::OutOfGas)?;
		if usage > self.limit {
			return Err(ExitError::OutOfGas);
		}
		self.usage = usage;
		Ok(())
	}

	fn refund(&mut self, amount: T) {
		self.usage = self.usage.saturating_sub(amount);
	}

	fn record(&mut self, opcode: Opcode, gas_cost: GasCost) -> Result<(), ExitError> {
		Ok(())
	}
}

pub struct StorageMetric(BasicMetric<u64>);

impl StorageMetric {
	fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}

	fn record_dynamic_opcode_cost(
		&mut self,
		_opcode: Opcode,
		gas_cost: GasCost,
	) -> Result<(), ExitError> {
		let cost = match gas_cost {
			GasCost::Create => 0,
			GasCost::Create2 { len } => len.try_into().map_err(|_| ExitError::OutOfGas)?,
			_ => return Ok(()),
		};
		self.0.try_consume(cost)
	}
}
