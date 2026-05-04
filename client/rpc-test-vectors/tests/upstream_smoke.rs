//! Sanity check: parse every `.io` file in the vendored execution-apis
//! `tests/` tree. Catches format drift on submodule bumps without needing a
//! live RPC endpoint.

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use fc_rpc_test_vectors::parse;

fn vendor_tests() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/execution-apis/tests")
}

#[test]
fn every_vendored_io_file_parses() {
	let root = vendor_tests();
	if !root.exists() {
		// Submodule not initialized — skip rather than fail. The instruction
		// to initialize lives in the crate README and CI.
		eprintln!("skipping: submodule not initialized at {}", root.display());
		return;
	}
	let mut total = 0;
	let mut failures: Vec<(PathBuf, String)> = Vec::new();
	walk(&root, &mut |path| {
		if path.extension() == Some(OsStr::new("io")) {
			total += 1;
			let raw = fs::read_to_string(path).expect("read .io");
			if let Err(err) = parse(&raw) {
				failures.push((path.to_path_buf(), err.to_string()));
			}
		}
	});
	assert!(total > 0, "expected at least one .io file under {}", root.display());
	assert!(
		failures.is_empty(),
		"failed to parse {} of {} vectors:\n{}",
		failures.len(),
		total,
		failures
			.iter()
			.map(|(p, e)| format!("  {}: {e}", p.display()))
			.collect::<Vec<_>>()
			.join("\n"),
	);
}

fn walk(dir: &std::path::Path, f: &mut dyn FnMut(&std::path::Path)) {
	for entry in fs::read_dir(dir).unwrap().flatten() {
		let path = entry.path();
		if path.is_dir() {
			walk(&path, f);
		} else {
			f(&path);
		}
	}
}
