//! Replay every applicable execution-apis vector against a
//! `frontier-template-node --dev` subprocess in schema-only mode.
//! Only vectors whose method matches an entry in `EXCLUDED_NAMESPACES`
//! (`testing_`, `engine_`) are skipped.
//!
//! Gated behind the `e2e` Cargo feature so a default `cargo test` skips
//! compilation entirely. Build the node binary, then run with the feature on:
//!
//! ```bash
//! cargo build -p frontier-template-node --release
//! cargo test -p fc-rpc-test-vectors --features e2e --test replay_template_node -- --nocapture
//! ```
//!
//! Override the binary location with `FRONTIER_NODE_BIN=/path/to/node`.

use std::env;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use fc_rpc_test_vectors::{
	run, CompareMode, HttpTransport, RunOutcome, Transport, EXCLUDED_NAMESPACES,
};
use serde_json::json;

const READY_TIMEOUT: Duration = Duration::from_secs(60);

#[test]
fn replay_execution_apis_vectors_against_template_node() {
	let node = TemplateNode::spawn();
	let transport = HttpTransport::new(node.rpc_url());

	let reports = run(
		&vendor_tests_dir(),
		&transport,
		EXCLUDED_NAMESPACES,
		&CompareMode::Schema,
	);

	let (failures, ok): (Vec<_>, Vec<_>) = reports.iter().partition(|r| r.is_failure());
	let attempted = ok
		.iter()
		.filter(|r| !matches!(r.outcome, RunOutcome::Skipped { .. }))
		.count();
	eprintln!(
		"vectors: {} attempted, {} skipped, {} failed",
		attempted,
		ok.len() - attempted,
		failures.len(),
	);
	for f in &failures {
		eprintln!("FAIL {}/{}: {:?}", f.method, f.case, f.outcome);
	}
	assert!(failures.is_empty(), "{} failure(s)", failures.len());
	assert!(attempted > 0, "no vectors were attempted");
}

fn vendor_tests_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/execution-apis/tests")
}

fn locate_node_binary() -> PathBuf {
	if let Ok(p) = env::var("FRONTIER_NODE_BIN") {
		let p = PathBuf::from(p);
		assert!(
			p.exists(),
			"FRONTIER_NODE_BIN does not exist: {}",
			p.display()
		);
		return p;
	}
	let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let target = manifest
		.parent()
		.and_then(Path::parent)
		.expect("workspace root")
		.join("target");
	for profile in ["release", "debug"] {
		let p = target.join(profile).join(node_bin_name());
		if p.exists() {
			return p;
		}
	}
	panic!(
		"frontier-template-node binary not found under {}/{{release,debug}}; build it first or set FRONTIER_NODE_BIN",
		target.display()
	);
}

fn node_bin_name() -> &'static str {
	if cfg!(windows) {
		"frontier-template-node.exe"
	} else {
		"frontier-template-node"
	}
}

struct TemplateNode {
	child: Child,
	rpc_port: u16,
	_base_path: TempDir,
}

impl TemplateNode {
	fn spawn() -> Self {
		let bin = locate_node_binary();
		let rpc_port = pick_free_port();
		let base_path = TempDir::new("fc-rpc-test-vectors-node");

		let mut cmd = Command::new(&bin);
		cmd.args([
			"--dev",
			"--rpc-port",
			&rpc_port.to_string(),
			"--rpc-cors=all",
			"--no-prometheus",
			"--no-telemetry",
			"--base-path",
		])
		.arg(base_path.path())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());

		let mut child = cmd
			.spawn()
			.unwrap_or_else(|e| panic!("failed to spawn {}: {e}", bin.display()));

		// Drain stdout/stderr to avoid pipe-fill deadlocks. Keep a copy in
		// memory so we can dump it on failure.
		drain(child.stdout.take().expect("stdout"), "node-stdout");
		drain(child.stderr.take().expect("stderr"), "node-stderr");

		wait_until_ready(rpc_port, &mut child);

		Self {
			child,
			rpc_port,
			_base_path: base_path,
		}
	}

	fn rpc_url(&self) -> String {
		format!("http://127.0.0.1:{}", self.rpc_port)
	}
}

impl Drop for TemplateNode {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}

fn pick_free_port() -> u16 {
	let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
	listener.local_addr().expect("local addr").port()
}

fn drain<R: std::io::Read + Send + 'static>(reader: R, tag: &'static str) {
	thread::spawn(move || {
		let buf = BufReader::new(reader);
		for line in buf.lines().map_while(Result::ok) {
			eprintln!("[{tag}] {line}");
		}
	});
}

fn wait_until_ready(port: u16, child: &mut Child) {
	let url = format!("http://127.0.0.1:{port}");
	let probe = HttpTransport::new(&url);
	let req = json!({"jsonrpc":"2.0","id":1,"method":"eth_chainId"});
	let deadline = Instant::now() + READY_TIMEOUT;
	loop {
		if Instant::now() >= deadline {
			let _ = child.kill();
			panic!("template node did not become ready within {READY_TIMEOUT:?}");
		}
		if let Some(status) = child.try_wait().expect("try_wait") {
			panic!("template node exited before ready: {status}");
		}
		if probe.send(&req).is_ok() {
			return;
		}
		thread::sleep(Duration::from_millis(250));
	}
}

struct TempDir(PathBuf);

impl TempDir {
	fn new(prefix: &str) -> Self {
		use std::sync::atomic::{AtomicU32, Ordering};
		static COUNTER: AtomicU32 = AtomicU32::new(0);
		let n = COUNTER.fetch_add(1, Ordering::Relaxed);
		let path = std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()));
		std::fs::create_dir_all(&path).expect("mkdir temp");
		Self(path)
	}

	fn path(&self) -> &OsStr {
		self.0.as_os_str()
	}
}

impl Drop for TempDir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
