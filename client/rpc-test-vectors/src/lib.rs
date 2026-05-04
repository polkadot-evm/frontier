//! Replay JSON-RPC test vectors from `ethereum/execution-apis` against any
//! HTTP RPC endpoint.
//!
//! Vector format (rpctestgen line-delimited):
//!
//! ```text
//! // optional comment
//! >> {"jsonrpc":"2.0","id":1,"method":"eth_blockNumber"}
//! << {"jsonrpc":"2.0","id":1,"result":"0x2d"}
//! ```
//!
//! See `docs/adr/001-rpc-test-vectors.md` for design context.
//!
//! # Using as a library from another workspace
//!
//! Downstream consumers (e.g. Moonbeam, custom Frontier-based devnets) can
//! depend on this crate via git and run vectors against their own nodes:
//!
//! ```toml
//! # Cargo.toml
//! [dev-dependencies]
//! fc-rpc-test-vectors = { git = "https://github.com/polkadot-evm/frontier", rev = "<sha>" }
//! ```
//!
//! The consumer is responsible for sourcing vectors (typically a submodule of
//! `ethereum/execution-apis`) and pointing the runner at them:
//!
//! ```no_run
//! use std::path::PathBuf;
//! use fc_rpc_test_vectors::{run, CompareMode, HttpTransport, CURATED_SUBSET};
//!
//! let tests_dir = PathBuf::from("vendor/execution-apis/tests");
//! let transport = HttpTransport::new("http://127.0.0.1:9944");
//! let reports = run(&tests_dir, &transport, CURATED_SUBSET, &CompareMode::Schema);
//! let failures: Vec<_> = reports.iter().filter(|r| r.is_failure()).collect();
//! assert!(failures.is_empty(), "{} failure(s)", failures.len());
//! ```
//!
//! [`CURATED_SUBSET`] is the conservative method allow-list this crate runs in
//! its own CI; consumers can pass a different slice.
//!
//! [`Transport`] is a trait — substitute any HTTP client by implementing it
//! directly. [`HttpTransport`] is provided as a small `ureq`-based default.

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
