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

pub use compare::{compare, DynamicFields, MatchOutcome};
pub use parser::{parse, Exchange, ParseError, Vector};
pub use runner::{run, RunOutcome, RunReport, Transport};
