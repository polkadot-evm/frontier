//! Replay JSON-RPC test vectors from `ethereum/execution-apis` against a
//! Frontier RPC endpoint.
//!
//! Vectors live under `vendor/execution-apis/tests/{method}/{name}.io` in the
//! rpctestgen line-delimited format:
//!
//! ```text
//! // optional comment
//! >> {"jsonrpc":"2.0","id":1,"method":"eth_blockNumber"}
//! << {"jsonrpc":"2.0","id":1,"result":"0x2d"}
//! ```
//!
//! See `docs/adr/001-rpc-test-vectors.md` for design context.

pub mod compare;
pub mod parser;
pub mod runner;
pub mod transport;

pub use compare::{compare, CompareMode, DynamicFields, MatchOutcome};
pub use parser::{parse, Exchange, ParseError, Vector};
pub use runner::{run, RunOutcome, RunReport, Transport};
pub use transport::HttpTransport;

/// Curated subset of methods that the runner exercises against a generic
/// Frontier-based dev node. Everything outside this set is reported as
/// `Skipped`. Expand as compatibility allows.
pub const CURATED_SUBSET: &[&str] = &[
	"eth_blockNumber",
	"eth_chainId",
	"eth_getBalance",
	"eth_getCode",
	"eth_getStorageAt",
	"eth_getBlockByNumber",
	"eth_getBlockByHash",
];
