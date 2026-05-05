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
//! use fc_rpc_test_vectors::{run, CompareMode, HttpTransport, SkipList, EXCLUDED_NAMESPACES};
//!
//! let tests_dir = PathBuf::from("vendor/execution-apis/tests");
//! let transport = HttpTransport::new("http://127.0.0.1:9944");
//! let skip_list = SkipList::new(); // or SkipList::from_file("path/to/skip.txt")?
//! let reports = run(&tests_dir, &transport, EXCLUDED_NAMESPACES, &skip_list, &CompareMode::Schema);
//! let failures: Vec<_> = reports.iter().filter(|r| r.is_failure()).collect();
//! assert!(failures.is_empty(), "{} failure(s)", failures.len());
//! ```
//!
//! [`EXCLUDED_NAMESPACES`] skips the JSON-RPC namespaces Frontier doesn't
//! claim to implement (`testing_`, `engine_`); consumers can pass a different
//! slice — e.g. an empty slice runs everything, or add custom prefixes for a
//! Frontier fork that disables additional namespaces.
//!
//! [`Transport`] is a trait — substitute any HTTP client by implementing it
//! directly. [`HttpTransport`] is provided as a small `ureq`-based default.

pub mod compare;
pub mod parser;
pub mod runner;
pub mod transport;

pub use compare::{compare, CompareMode, DynamicFields, MatchOutcome};
pub use parser::{parse, Exchange, ParseError, Vector};
pub use runner::{run, RunOutcome, RunReport, SkipList, Transport};
pub use transport::HttpTransport;

/// JSON-RPC method-name prefixes the runner skips by default. These are
/// namespaces Frontier (and Frontier-based clients) do not claim to implement:
///
/// - `testing_` — Hive-only test API, not part of standard EL JSON-RPC.
/// - `engine_` — consensus-engine API, served by the CL in PoS Ethereum.
///
/// Pass an empty slice to `run` if you want to surface failures for these too.
pub const EXCLUDED_NAMESPACES: &[&str] = &["testing_", "engine_"];
