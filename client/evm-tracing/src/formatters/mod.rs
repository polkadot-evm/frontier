pub mod blockscout;
pub mod call_tracer;
pub mod raw;
pub mod trace_filter;

pub use blockscout::Formatter as Blockscout;
pub use call_tracer::Formatter as CallTracer;
pub use raw::Formatter as Raw;
pub use trace_filter::Formatter as TraceFilter;

use evm_tracing_events::Listener;
use serde::Serialize;

pub trait ResponseFormatter {
	type Listener: Listener;
	type Response: Serialize;

	fn format(listener: Self::Listener) -> Option<Self::Response>;
}
