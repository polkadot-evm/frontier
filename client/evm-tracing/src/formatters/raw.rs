use crate::listeners::raw::Listener;
use crate::types::single::TransactionTrace;

pub struct Formatter;

impl super::ResponseFormatter for Formatter {
	type Listener = Listener;
	type Response = TransactionTrace;

	fn format(listener: Listener) -> Option<TransactionTrace> {
		if listener.remaining_memory_usage.is_none() {
			None
		} else {
			Some(TransactionTrace::Raw {
				step_logs: listener.step_logs,
				gas: listener.final_gas.into(),
				return_value: listener.return_value,
			})
		}
	}
}

