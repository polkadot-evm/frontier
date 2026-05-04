//! Walk a vendored `tests/` directory, replay each enabled vector through a
//! `Transport`, and report per-file outcomes.
//!
//! Transport is left as a trait so the crate stays free of HTTP / runtime
//! dependencies. Real test binaries plug in their own transport (subprocess
//! HTTP, in-process server, etc.).

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::compare::{compare, DynamicFields, MatchOutcome};
use crate::parser::{parse, ParseError};

pub trait Transport {
	fn send(&self, request: &Value) -> Result<Value, String>;
}

#[derive(Debug)]
pub enum RunOutcome {
	Match,
	Skipped { reason: &'static str },
	SchemaOnly,
	Mismatch { path: String, detail: String },
	TransportError(String),
	ParseError(ParseError),
	IoError(io::Error),
}

#[derive(Debug)]
pub struct RunReport {
	pub file: PathBuf,
	pub method: String,
	pub case: String,
	pub outcome: RunOutcome,
}

impl RunReport {
	pub fn is_failure(&self) -> bool {
		matches!(
			self.outcome,
			RunOutcome::Mismatch { .. }
				| RunOutcome::TransportError(_)
				| RunOutcome::ParseError(_)
				| RunOutcome::IoError(_)
		)
	}
}

/// Replay every `.io` file under `tests_dir`. Vectors whose method is not in
/// `enabled_methods` are reported as `Skipped`, not as failures — this keeps
/// the runner green while coverage grows.
pub fn run<T: Transport>(
	tests_dir: &Path,
	transport: &T,
	enabled_methods: &[&str],
	dynamic: &DynamicFields,
) -> Vec<RunReport> {
	let mut reports = Vec::new();
	collect(tests_dir, &mut reports, transport, enabled_methods, dynamic);
	reports
}

fn collect<T: Transport>(
	dir: &Path,
	reports: &mut Vec<RunReport>,
	transport: &T,
	enabled: &[&str],
	dynamic: &DynamicFields,
) {
	let entries = match fs::read_dir(dir) {
		Ok(e) => e,
		Err(err) => {
			reports.push(RunReport {
				file: dir.to_path_buf(),
				method: String::new(),
				case: String::new(),
				outcome: RunOutcome::IoError(err),
			});
			return;
		}
	};

	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_dir() {
			collect(&path, reports, transport, enabled, dynamic);
		} else if path.extension() == Some(OsStr::new("io")) {
			reports.push(replay_file(&path, transport, enabled, dynamic));
		}
	}
}

fn replay_file<T: Transport>(
	path: &Path,
	transport: &T,
	enabled: &[&str],
	dynamic: &DynamicFields,
) -> RunReport {
	let method = path
		.parent()
		.and_then(Path::file_name)
		.and_then(OsStr::to_str)
		.unwrap_or("")
		.to_string();
	let case = path
		.file_stem()
		.and_then(OsStr::to_str)
		.unwrap_or("")
		.to_string();
	let mk = |outcome| RunReport {
		file: path.to_path_buf(),
		method: method.clone(),
		case: case.clone(),
		outcome,
	};

	if !enabled.iter().any(|m| *m == method) {
		return mk(RunOutcome::Skipped {
			reason: "method not in enabled set",
		});
	}

	let raw = match fs::read_to_string(path) {
		Ok(s) => s,
		Err(err) => return mk(RunOutcome::IoError(err)),
	};
	let vector = match parse(&raw) {
		Ok(v) => v,
		Err(err) => return mk(RunOutcome::ParseError(err)),
	};

	for exchange in &vector.exchanges {
		match transport.send(&exchange.request) {
			Ok(actual) => {
				if vector.speconly {
					return mk(RunOutcome::SchemaOnly);
				}
				if let MatchOutcome::Mismatch { path, detail } =
					compare(&exchange.response, &actual, dynamic)
				{
					return mk(RunOutcome::Mismatch { path, detail });
				}
			}
			Err(err) => return mk(RunOutcome::TransportError(err)),
		}
	}

	mk(RunOutcome::Match)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	use std::cell::RefCell;
	use std::io::Write;

	struct StubTransport {
		response: Value,
		seen: RefCell<Vec<Value>>,
	}

	impl Transport for StubTransport {
		fn send(&self, request: &Value) -> Result<Value, String> {
			self.seen.borrow_mut().push(request.clone());
			Ok(self.response.clone())
		}
	}

	fn write_vector(dir: &Path, method: &str, case: &str, body: &str) -> PathBuf {
		let method_dir = dir.join(method);
		fs::create_dir_all(&method_dir).unwrap();
		let path = method_dir.join(format!("{case}.io"));
		let mut f = fs::File::create(&path).unwrap();
		f.write_all(body.as_bytes()).unwrap();
		path
	}

	#[test]
	fn matches_when_response_equal() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_blockNumber",
			"simple",
			">> {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_blockNumber\"}\n\
			 << {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"jsonrpc":"2.0","id":1,"result":"0x1"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(
			tmp.path(),
			&t,
			&["eth_blockNumber"],
			&DynamicFields::default(),
		);
		assert_eq!(reports.len(), 1);
		assert!(matches!(reports[0].outcome, RunOutcome::Match));
		assert_eq!(t.seen.borrow().len(), 1);
	}

	#[test]
	fn skips_methods_not_in_enabled_set() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_someUnsupported",
			"x",
			">> {\"id\":1}\n<< {\"id\":1,\"r\":1}\n",
		);
		let t = StubTransport {
			response: json!({}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &["eth_blockNumber"], &DynamicFields::default());
		assert_eq!(reports.len(), 1);
		assert!(matches!(reports[0].outcome, RunOutcome::Skipped { .. }));
		assert!(t.seen.borrow().is_empty());
	}

	#[test]
	fn flags_mismatch() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_blockNumber",
			"simple",
			">> {\"id\":1}\n<< {\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"id":1,"result":"0x2"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(
			tmp.path(),
			&t,
			&["eth_blockNumber"],
			&DynamicFields::default(),
		);
		assert!(matches!(reports[0].outcome, RunOutcome::Mismatch { .. }));
		assert!(reports[0].is_failure());
	}

	#[test]
	fn speconly_does_not_compare_values() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_blockNumber",
			"simple",
			"// speconly\n>> {\"id\":1}\n<< {\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"id":1,"result":"0xdeadbeef"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(
			tmp.path(),
			&t,
			&["eth_blockNumber"],
			&DynamicFields::default(),
		);
		assert!(matches!(reports[0].outcome, RunOutcome::SchemaOnly));
	}

	// ---- minimal tempdir helper, to avoid pulling in a tempfile dep ----

	struct TempDir(PathBuf);

	impl TempDir {
		fn path(&self) -> &Path {
			&self.0
		}
	}

	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	fn tempdir() -> TempDir {
		use std::sync::atomic::{AtomicU32, Ordering};
		static COUNTER: AtomicU32 = AtomicU32::new(0);
		let n = COUNTER.fetch_add(1, Ordering::Relaxed);
		let path = std::env::temp_dir().join(format!(
			"fc-rpc-test-vectors-{}-{}",
			std::process::id(),
			n
		));
		fs::create_dir_all(&path).unwrap();
		TempDir(path)
	}
}
