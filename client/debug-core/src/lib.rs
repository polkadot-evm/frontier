use ethereum_types::H256;
use futures::future::BoxFuture;
use jsonrpc_core::Result as RpcResult;
use jsonrpc_derive::rpc;
use fc_evm_tracing::types::single;
use fc_rpc_core::types::RequestBlockId;
use serde::Deserialize;

pub use rpc_impl_Debug::gen_server::Debug as DebugServer;

#[derive(Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceParams {
	pub disable_storage: Option<bool>,
	pub disable_memory: Option<bool>,
	pub disable_stack: Option<bool>,
	/// Javascript tracer (we just check if it's Blockscout tracer string)
	pub tracer: Option<String>,
	pub timeout: Option<String>,
}

#[rpc(server)]
pub trait Debug {
	#[rpc(name = "debug_traceTransaction")]
	fn trace_transaction(
		&self,
		transaction_hash: H256,
		params: Option<TraceParams>,
	) -> BoxFuture<'static, RpcResult<single::TransactionTrace>>;
	#[rpc(name = "debug_traceBlockByNumber", alias("debug_traceBlockByHash"))]
	fn trace_block(
		&self,
		id: RequestBlockId,
		params: Option<TraceParams>,
	) -> BoxFuture<'static, RpcResult<Vec<single::TransactionTrace>>>;
}
