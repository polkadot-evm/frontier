//! Replay the curated subset of execution-apis vectors against any
//! HTTP JSON-RPC endpoint. Useful for ad-hoc validation against a Moonbeam
//! dev node, a custom Frontier-based devnet, etc.
//!
//! ```bash
//! cargo run -p fc-rpc-test-vectors --example replay_url -- --url http://127.0.0.1:9944
//! ```
//!
//! Schema-only mode by default (chain state will not match the upstream Hive
//! chain). Pass `--exact` to require value matches — only useful when pointed
//! at an actual geth/reth node running the Hive chain.

use std::path::PathBuf;
use std::process::ExitCode;

use fc_rpc_test_vectors::{
	run, CompareMode, DynamicFields, HttpTransport, RunOutcome, CURATED_SUBSET,
};

fn main() -> ExitCode {
	let args = match Args::parse() {
		Ok(a) => a,
		Err(e) => {
			eprintln!("error: {e}\n\n{USAGE}");
			return ExitCode::from(2);
		}
	};

	let tests_dir = args.tests_dir.unwrap_or_else(|| {
		PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/execution-apis/tests")
	});
	if !tests_dir.exists() {
		eprintln!("tests directory not found: {}", tests_dir.display());
		eprintln!("hint: `git submodule update --init --recursive`");
		return ExitCode::from(2);
	}

	let transport = HttpTransport::new(args.url);
	let mode = if args.exact {
		CompareMode::Exact(DynamicFields::defaults())
	} else {
		CompareMode::Schema
	};

	let reports = run(&tests_dir, &transport, CURATED_SUBSET, &mode);

	let mut attempted = 0usize;
	let mut skipped = 0usize;
	let mut failures = 0usize;
	for r in &reports {
		match &r.outcome {
			RunOutcome::Skipped { .. } => skipped += 1,
			RunOutcome::Match | RunOutcome::SchemaOnly => attempted += 1,
			_ => {
				attempted += 1;
				failures += 1;
				eprintln!("FAIL {}/{}: {:?}", r.method, r.case, r.outcome);
			}
		}
	}
	eprintln!("attempted={attempted} skipped={skipped} failures={failures}");
	if failures == 0 {
		ExitCode::SUCCESS
	} else {
		ExitCode::FAILURE
	}
}

const USAGE: &str = "\
usage: replay_url --url <URL> [--exact] [--tests-dir <PATH>]

  --url <URL>          HTTP JSON-RPC endpoint to replay against (required)
  --exact              Require exact value matches (default: schema-only)
  --tests-dir <PATH>   Override path to execution-apis tests/ directory
";

struct Args {
	url: String,
	exact: bool,
	tests_dir: Option<PathBuf>,
}

impl Args {
	fn parse() -> Result<Self, String> {
		let mut url: Option<String> = None;
		let mut exact = false;
		let mut tests_dir: Option<PathBuf> = None;
		let mut it = std::env::args().skip(1);
		while let Some(arg) = it.next() {
			match arg.as_str() {
				"--url" => url = Some(it.next().ok_or("--url needs a value")?),
				"--exact" => exact = true,
				"--tests-dir" => {
					tests_dir = Some(PathBuf::from(it.next().ok_or("--tests-dir needs a value")?));
				}
				"-h" | "--help" => {
					println!("{USAGE}");
					std::process::exit(0);
				}
				other => return Err(format!("unknown argument: {other}")),
			}
		}
		Ok(Self {
			url: url.ok_or("--url is required")?,
			exact,
			tests_dir,
		})
	}
}
