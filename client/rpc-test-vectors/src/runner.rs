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

use crate::compare::{compare, CompareMode, MatchOutcome};
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
	EnvelopeError(String),
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
				| RunOutcome::EnvelopeError(_)
				| RunOutcome::TransportError(_)
				| RunOutcome::ParseError(_)
				| RunOutcome::IoError(_)
		)
	}
}

/// Replay every `.io` file under `tests_dir`. Vectors whose method starts with
/// any prefix in `excluded_prefixes` are reported as `Skipped`, not as
/// failures — used to skip namespaces Frontier doesn't claim to implement
/// (e.g. `testing_`, `engine_`).
pub fn run<T: Transport>(
	tests_dir: &Path,
	transport: &T,
	excluded_prefixes: &[&str],
	mode: &CompareMode,
) -> Vec<RunReport> {
	let mut reports = Vec::new();
	collect(tests_dir, &mut reports, transport, excluded_prefixes, mode);
	reports
}

fn collect<T: Transport>(
	dir: &Path,
	reports: &mut Vec<RunReport>,
	transport: &T,
	excluded: &[&str],
	mode: &CompareMode,
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
			collect(&path, reports, transport, excluded, mode);
		} else if path.extension() == Some(OsStr::new("io")) {
			reports.push(replay_file(&path, transport, excluded, mode));
		}
	}
}

/// Validate the JSON-RPC envelope independently of vector content:
/// `jsonrpc=="2.0"`, `id` matches the request `id`, exactly one of `result` or
/// `error` is present.
fn validate_envelope(request: &Value, actual: &Value) -> Result<(), String> {
	let obj = actual
		.as_object()
		.ok_or_else(|| format!("response is not a JSON object: {actual}"))?;
	match obj.get("jsonrpc") {
		Some(Value::String(v)) if v == "2.0" => {}
		other => return Err(format!("expected jsonrpc=\"2.0\", got {other:?}")),
	}
	let req_id = request.get("id");
	let res_id = obj.get("id");
	if req_id != res_id {
		return Err(format!(
			"id mismatch: request {req_id:?}, response {res_id:?}"
		));
	}
	match (obj.contains_key("result"), obj.contains_key("error")) {
		(true, false) | (false, true) => Ok(()),
		(true, true) => Err("response has both result and error".to_string()),
		(false, false) => Err("response has neither result nor error".to_string()),
	}
}

fn replay_file<T: Transport>(
	path: &Path,
	transport: &T,
	excluded: &[&str],
	mode: &CompareMode,
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

	if excluded.iter().any(|p| method.starts_with(p)) {
		return mk(RunOutcome::Skipped {
			reason: "method namespace excluded",
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
				if let Err(err) = validate_envelope(&exchange.request, &actual) {
					return mk(RunOutcome::EnvelopeError(err));
				}
				if vector.speconly {
					return mk(RunOutcome::SchemaOnly);
				}
				if let MatchOutcome::Mismatch { path, detail } =
					compare(&exchange.response, &actual, mode)
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
	use crate::compare::DynamicFields;
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

	fn exact() -> CompareMode {
		CompareMode::Exact(DynamicFields::default())
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
			">> {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_blockNumber\"}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"jsonrpc":"2.0","id":1,"result":"0x1"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &[], &exact());
		assert_eq!(reports.len(), 1);
		assert!(matches!(reports[0].outcome, RunOutcome::Match));
		assert_eq!(t.seen.borrow().len(), 1);
	}

	#[test]
	fn skips_methods_matching_excluded_prefix() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"testing_buildBlockV1",
			"x",
			">> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":1}\n",
		);
		let t = StubTransport {
			response: json!({}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &["testing_"], &exact());
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
			">> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"jsonrpc":"2.0","id":1,"result":"0x2"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &[], &exact());
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
			"// speconly\n>> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"jsonrpc":"2.0","id":1,"result":"0xdeadbeef"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &[], &exact());
		assert!(matches!(reports[0].outcome, RunOutcome::SchemaOnly));
	}

	#[test]
	fn flags_envelope_id_mismatch() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_blockNumber",
			"simple",
			">> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"jsonrpc":"2.0","id":99,"result":"0x1"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &[], &CompareMode::Schema);
		assert!(matches!(reports[0].outcome, RunOutcome::EnvelopeError(_)));
	}

	#[test]
	fn flags_envelope_missing_jsonrpc_field() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_blockNumber",
			"simple",
			">> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"id":1,"result":"0x1"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &[], &CompareMode::Schema);
		assert!(matches!(reports[0].outcome, RunOutcome::EnvelopeError(_)));
	}

	#[test]
	fn schema_mode_tolerates_value_diffs() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_blockNumber",
			"simple",
			">> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"jsonrpc":"2.0","id":1,"result":"0xdeadbeef"}),
			seen: RefCell::new(vec![]),
		};
		let reports = run(tmp.path(), &t, &[], &CompareMode::Schema);
		assert!(matches!(reports[0].outcome, RunOutcome::Match));
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
		let path =
			std::env::temp_dir().join(format!("fc-rpc-test-vectors-{}-{}", std::process::id(), n));
		fs::create_dir_all(&path).unwrap();
		TempDir(path)
	}
}
