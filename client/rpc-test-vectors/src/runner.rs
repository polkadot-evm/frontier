//! Walk a vendored `tests/` directory, replay each enabled vector through a
//! `Transport`, and report per-file outcomes.
//!
//! Transport is left as a trait so the crate stays free of HTTP / runtime
//! dependencies. Real test binaries plug in their own transport (subprocess
//! HTTP, in-process server, etc.).

use std::collections::HashMap;
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
	Skipped { reason: String },
	SchemaOnly,
	Mismatch { path: String, detail: String },
	EnvelopeError(String),
	TransportError(String),
	ParseError(ParseError),
	IoError(io::Error),
}

/// Per-vector skip list, keyed by `(method, case)`. Matched vectors are
/// reported as `Skipped { reason }` instead of being replayed.
///
/// Constructed from a flat text file shipped alongside the crate or the
/// vendored vectors:
///
/// ```text
/// # blank lines and `# comment` lines are ignored
/// eth_getBlockByNumber/get-genesis  # missing mixHash on Substrate genesis
/// ```
///
/// The format is intentionally case-level only — globbing would silently
/// auto-skip new upstream vectors for the same method, which is the
/// opposite of what we want.
#[derive(Debug, Default, Clone)]
pub struct SkipList {
	entries: HashMap<(String, String), String>,
}

impl SkipList {
	pub fn new() -> Self {
		Self::default()
	}

	/// Parse a skip-list text file. Returns `io::Error` if the file is
	/// unreadable; malformed lines are silently ignored so the test stays
	/// usable while the file is being edited.
	pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
		Ok(Self::from_text(&fs::read_to_string(path.as_ref())?))
	}

	pub fn from_text(text: &str) -> Self {
		let mut entries = HashMap::new();
		for line in text.lines() {
			let trimmed = line.trim();
			if trimmed.is_empty() || trimmed.starts_with('#') {
				continue;
			}
			let (entry, reason) = match trimmed.split_once('#') {
				Some((e, r)) => (e.trim(), r.trim().to_string()),
				None => (trimmed, String::new()),
			};
			if let Some((method, case)) = entry.split_once('/') {
				entries.insert((method.trim().to_string(), case.trim().to_string()), reason);
			}
		}
		Self { entries }
	}

	pub fn lookup(&self, method: &str, case: &str) -> Option<&str> {
		self.entries
			.get(&(method.to_string(), case.to_string()))
			.map(String::as_str)
	}

	pub fn len(&self) -> usize {
		self.entries.len()
	}

	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}
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
/// (e.g. `testing_`, `engine_`). Vectors matched by `skip_list` are also
/// reported as `Skipped`, with the reason from the file.
pub fn run<T: Transport>(
	tests_dir: &Path,
	transport: &T,
	excluded_prefixes: &[&str],
	skip_list: &SkipList,
	mode: &CompareMode,
) -> Vec<RunReport> {
	let mut reports = Vec::new();
	collect(
		tests_dir,
		&mut reports,
		transport,
		excluded_prefixes,
		skip_list,
		mode,
	);
	reports
}

fn collect<T: Transport>(
	dir: &Path,
	reports: &mut Vec<RunReport>,
	transport: &T,
	excluded: &[&str],
	skip_list: &SkipList,
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

	for entry in entries {
		let entry = match entry {
			Ok(e) => e,
			Err(err) => {
				reports.push(RunReport {
					file: dir.to_path_buf(),
					method: String::new(),
					case: String::new(),
					outcome: RunOutcome::IoError(err),
				});
				continue;
			}
		};
		let path = entry.path();
		if path.is_dir() {
			collect(&path, reports, transport, excluded, skip_list, mode);
		} else if path.extension() == Some(OsStr::new("io")) {
			reports.push(replay_file(&path, transport, excluded, skip_list, mode));
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
	skip_list: &SkipList,
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
			reason: "method namespace excluded".to_string(),
		});
	}

	if let Some(reason) = skip_list.lookup(&method, &case) {
		return mk(RunOutcome::Skipped {
			reason: reason.to_string(),
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

	let speconly_mode = CompareMode::Schema;
	let cmp_mode = if vector.speconly {
		&speconly_mode
	} else {
		mode
	};

	for exchange in &vector.exchanges {
		match transport.send(&exchange.request) {
			Ok(actual) => {
				if let Err(err) = validate_envelope(&exchange.request, &actual) {
					return mk(RunOutcome::EnvelopeError(err));
				}
				if let MatchOutcome::Mismatch { path, detail } =
					compare(&exchange.response, &actual, cmp_mode)
				{
					return mk(RunOutcome::Mismatch { path, detail });
				}
			}
			Err(err) => return mk(RunOutcome::TransportError(err)),
		}
	}

	if vector.speconly {
		mk(RunOutcome::SchemaOnly)
	} else {
		mk(RunOutcome::Match)
	}
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
		let reports = run(tmp.path(), &t, &[], &SkipList::new(), &exact());
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
		let reports = run(tmp.path(), &t, &["testing_"], &SkipList::new(), &exact());
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
		let reports = run(tmp.path(), &t, &[], &SkipList::new(), &exact());
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
		let reports = run(tmp.path(), &t, &[], &SkipList::new(), &exact());
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
		let reports = run(tmp.path(), &t, &[], &SkipList::new(), &CompareMode::Schema);
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
		let reports = run(tmp.path(), &t, &[], &SkipList::new(), &CompareMode::Schema);
		assert!(matches!(reports[0].outcome, RunOutcome::EnvelopeError(_)));
	}

	#[test]
	fn skip_list_skips_listed_case_and_replays_others() {
		let tmp = tempdir();
		write_vector(
			tmp.path(),
			"eth_blockNumber",
			"simple",
			">> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		write_vector(
			tmp.path(),
			"eth_chainId",
			"basic",
			">> {\"jsonrpc\":\"2.0\",\"id\":1}\n<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n",
		);
		let t = StubTransport {
			response: json!({"jsonrpc":"2.0","id":1,"result":"0x1"}),
			seen: RefCell::new(vec![]),
		};
		let skip = SkipList::from_text(
			"# header comment\n\
			eth_blockNumber/simple  # known gap\n",
		);
		let reports = run(tmp.path(), &t, &[], &skip, &CompareMode::Schema);
		let by_method: std::collections::HashMap<_, _> = reports
			.iter()
			.map(|r| (r.method.as_str(), &r.outcome))
			.collect();
		assert!(matches!(
			by_method["eth_blockNumber"],
			RunOutcome::Skipped { reason } if reason == "known gap"
		));
		assert!(matches!(by_method["eth_chainId"], RunOutcome::Match));
	}

	#[test]
	fn skip_list_parses_inline_comments_and_blank_lines() {
		let text = concat!(
			"\n",
			"# this whole line is a comment\n",
			"\n",
			"eth_a/foo\n",
			"eth_b/bar  # with reason\n",
			"   eth_c/baz   #   trims whitespace   \n",
		);
		let s = SkipList::from_text(text);
		assert_eq!(s.len(), 3);
		assert_eq!(s.lookup("eth_a", "foo"), Some(""));
		assert_eq!(s.lookup("eth_b", "bar"), Some("with reason"));
		assert_eq!(s.lookup("eth_c", "baz"), Some("trims whitespace"));
		assert_eq!(s.lookup("eth_d", "missing"), None);
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
		let reports = run(tmp.path(), &t, &[], &SkipList::new(), &CompareMode::Schema);
		assert!(matches!(reports[0].outcome, RunOutcome::Match));
	}

	fn tempdir() -> tempfile::TempDir {
		tempfile::Builder::new()
			.prefix("fc-rpc-test-vectors-")
			.tempdir()
			.expect("should create temp directory")
	}
}
