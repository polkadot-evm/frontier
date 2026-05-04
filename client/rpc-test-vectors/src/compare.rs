//! Compare an actual JSON-RPC response against the expected one.
//!
//! Two modes:
//!
//! - [`CompareMode::Exact`] — values must match. A small allow-list of
//!   legitimately dynamic fields (block hashes, timestamps) is compared by
//!   shape only.
//! - [`CompareMode::Schema`] — only the JSON shape is checked: object keys
//!   in the expected response must exist in the actual response with matching
//!   primitive types, but values are not compared and array lengths and
//!   actual's extra fields are tolerated. Used when the runner replays
//!   upstream vectors against a chain whose state does not match the upstream
//!   test chain (the typical Frontier case — Substrate-based clients can't
//!   reproduce the geth Hive chain).
//!
//! The allow-list in `Exact` mode is intentionally small — if it grows, that
//! is a signal of real divergence rather than a fix.

use serde_json::Value;

/// Field names whose values should be compared for *type and presence* only,
/// not exact value. Matched anywhere in the JSON tree.
#[derive(Debug, Clone, Default)]
pub struct DynamicFields {
	names: Vec<String>,
}

impl DynamicFields {
	pub fn new<I, S>(names: I) -> Self
	where
		I: IntoIterator<Item = S>,
		S: Into<String>,
	{
		Self {
			names: names.into_iter().map(Into::into).collect(),
		}
	}

	/// Default set: timestamps and content-addressed hashes. These vary between
	/// chains with otherwise-identical semantics.
	pub fn defaults() -> Self {
		Self::new([
			"hash",
			"blockHash",
			"parentHash",
			"stateRoot",
			"receiptsRoot",
			"transactionsRoot",
			"sha3Uncles",
			"mixHash",
			"timestamp",
			"size",
			"extraData",
			"miner",
			"nonce",
		])
	}

	fn is_dynamic(&self, name: &str) -> bool {
		self.names.iter().any(|n| n == name)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchOutcome {
	Match,
	Mismatch { path: String, detail: String },
}

#[derive(Debug, Clone)]
pub enum CompareMode {
	/// Exact value match, with [`DynamicFields`] compared by shape only.
	Exact(DynamicFields),
	/// Shape-only match: object keys, primitive types. Values, array lengths,
	/// and actual's extra keys are not enforced.
	Schema,
}

pub fn compare(expected: &Value, actual: &Value, mode: &CompareMode) -> MatchOutcome {
	match mode {
		CompareMode::Exact(dynamic) => compare_at(expected, actual, dynamic, &mut Vec::new()),
		CompareMode::Schema => compare_schema_at(expected, actual, &mut Vec::new()),
	}
}

fn compare_at(
	expected: &Value,
	actual: &Value,
	dynamic: &DynamicFields,
	path: &mut Vec<String>,
) -> MatchOutcome {
	match (expected, actual) {
		(Value::Object(e), Value::Object(a)) => {
			for (key, e_val) in e {
				path.push(key.clone());
				let outcome = match a.get(key) {
					Some(a_val) => {
						if dynamic.is_dynamic(key) {
							compare_dynamic(e_val, a_val, path)
						} else {
							compare_at(e_val, a_val, dynamic, path)
						}
					}
					None => mismatch(path, format!("missing field; expected {e_val}")),
				};
				path.pop();
				if let MatchOutcome::Mismatch { .. } = outcome {
					return outcome;
				}
			}
			for key in a.keys() {
				if !e.contains_key(key) {
					path.push(key.clone());
					let outcome = mismatch(path, format!("unexpected field; got {}", a[key]));
					path.pop();
					return outcome;
				}
			}
			MatchOutcome::Match
		}
		(Value::Array(e), Value::Array(a)) => {
			if e.len() != a.len() {
				return mismatch(
					path,
					format!("array length differs: expected {}, got {}", e.len(), a.len()),
				);
			}
			for (idx, (e_item, a_item)) in e.iter().zip(a).enumerate() {
				path.push(format!("[{idx}]"));
				let outcome = compare_at(e_item, a_item, dynamic, path);
				path.pop();
				if let MatchOutcome::Mismatch { .. } = outcome {
					return outcome;
				}
			}
			MatchOutcome::Match
		}
		_ if expected == actual => MatchOutcome::Match,
		_ => mismatch(path, format!("expected {expected}, got {actual}")),
	}
}

/// For a dynamic field, only require that the JSON shape (string vs object vs
/// array vs null) matches. Empty/null are treated as compatible with any
/// concrete same-shape value to keep the rule predictable.
fn compare_dynamic(expected: &Value, actual: &Value, path: &mut [String]) -> MatchOutcome {
	if shape(expected) == shape(actual) {
		MatchOutcome::Match
	} else {
		mismatch(
			path,
			format!(
				"dynamic field shape differs: expected {}, got {}",
				shape(expected),
				shape(actual)
			),
		)
	}
}

fn compare_schema_at(expected: &Value, actual: &Value, path: &mut Vec<String>) -> MatchOutcome {
	match (expected, actual) {
		(Value::Object(e), Value::Object(a)) => {
			for (key, e_val) in e {
				path.push(key.clone());
				let outcome = match a.get(key) {
					Some(a_val) => compare_schema_at(e_val, a_val, path),
					None => mismatch(path, format!("missing field; expected shape {}", shape(e_val))),
				};
				path.pop();
				if let MatchOutcome::Mismatch { .. } = outcome {
					return outcome;
				}
			}
			MatchOutcome::Match
		}
		(Value::Array(e), Value::Array(a)) => {
			// Compare element shape pairwise up to min length. We do not
			// enforce length equality — chain state diverges on transaction
			// counts, log counts, etc. — but if both have a first element
			// we use it as a representative shape.
			for (idx, (e_item, a_item)) in e.iter().zip(a).enumerate() {
				path.push(format!("[{idx}]"));
				let outcome = compare_schema_at(e_item, a_item, path);
				path.pop();
				if let MatchOutcome::Mismatch { .. } = outcome {
					return outcome;
				}
			}
			MatchOutcome::Match
		}
		// Null is compatible with any shape — upstream vectors sometimes use
		// null for optional fields and the actual node may produce a real value.
		(Value::Null, _) | (_, Value::Null) => MatchOutcome::Match,
		_ if shape(expected) == shape(actual) => MatchOutcome::Match,
		_ => mismatch(
			path,
			format!(
				"shape differs: expected {}, got {}",
				shape(expected),
				shape(actual)
			),
		),
	}
}

fn shape(v: &Value) -> &'static str {
	match v {
		Value::Null => "null",
		Value::Bool(_) => "bool",
		Value::Number(_) => "number",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	}
}

fn mismatch(path: &[String], detail: String) -> MatchOutcome {
	let path = if path.is_empty() {
		"$".to_string()
	} else {
		format!("${}", join_path(path))
	};
	MatchOutcome::Mismatch { path, detail }
}

fn join_path(parts: &[String]) -> String {
	let mut out = String::new();
	for p in parts {
		if p.starts_with('[') {
			out.push_str(p);
		} else {
			out.push('.');
			out.push_str(p);
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn exact() -> CompareMode {
		CompareMode::Exact(DynamicFields::default())
	}

	#[test]
	fn matches_identical_objects() {
		let v = json!({"a":1,"b":[1,2,3]});
		assert_eq!(compare(&v, &v, &exact()), MatchOutcome::Match);
	}

	#[test]
	fn detects_value_mismatch() {
		let e = json!({"a":1});
		let a = json!({"a":2});
		let out = compare(&e, &a, &exact());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.a"));
	}

	#[test]
	fn detects_missing_field() {
		let e = json!({"a":1,"b":2});
		let a = json!({"a":1});
		let out = compare(&e, &a, &exact());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.b"));
	}

	#[test]
	fn detects_unexpected_field() {
		let e = json!({"a":1});
		let a = json!({"a":1,"b":2});
		let out = compare(&e, &a, &exact());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.b"));
	}

	#[test]
	fn dynamic_fields_only_check_shape() {
		let e = json!({"hash":"0xaaaa","number":"0x1"});
		let a = json!({"hash":"0xbbbb","number":"0x1"});
		assert_eq!(
			compare(&e, &a, &CompareMode::Exact(DynamicFields::new(["hash"]))),
			MatchOutcome::Match
		);
	}

	#[test]
	fn dynamic_fields_still_catch_shape_diffs() {
		let e = json!({"hash":"0xaaaa"});
		let a = json!({"hash":null});
		let out = compare(&e, &a, &CompareMode::Exact(DynamicFields::new(["hash"])));
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.hash"));
	}

	#[test]
	fn array_length_must_match() {
		let e = json!([1, 2, 3]);
		let a = json!([1, 2]);
		let out = compare(&e, &a, &exact());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$"));
	}

	#[test]
	fn nested_path_reported() {
		let e = json!({"result":{"items":[{"x":1}]}});
		let a = json!({"result":{"items":[{"x":2}]}});
		let out = compare(&e, &a, &exact());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.result.items[0].x"));
	}

	#[test]
	fn schema_ignores_value_differences() {
		let e = json!({"hash":"0xaaaa","number":"0x1"});
		let a = json!({"hash":"0xbbbb","number":"0xff"});
		assert_eq!(compare(&e, &a, &CompareMode::Schema), MatchOutcome::Match);
	}

	#[test]
	fn schema_catches_type_mismatch() {
		let e = json!({"number":"0x1"});
		let a = json!({"number":1});
		let out = compare(&e, &a, &CompareMode::Schema);
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.number"));
	}

	#[test]
	fn schema_catches_missing_field() {
		let e = json!({"a":1,"b":"x"});
		let a = json!({"a":1});
		let out = compare(&e, &a, &CompareMode::Schema);
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.b"));
	}

	#[test]
	fn schema_tolerates_extra_actual_fields() {
		let e = json!({"a":1});
		let a = json!({"a":2,"extra":"ok"});
		assert_eq!(compare(&e, &a, &CompareMode::Schema), MatchOutcome::Match);
	}

	#[test]
	fn schema_tolerates_array_length_diff() {
		let e = json!([{"x":"0x1"}]);
		let a = json!([{"x":"0x2"}, {"x":"0x3"}, {"x":"0x4"}]);
		assert_eq!(compare(&e, &a, &CompareMode::Schema), MatchOutcome::Match);
	}

	#[test]
	fn schema_treats_null_as_compatible() {
		// Optional fields are commonly null in vectors but populated by the node.
		let e = json!({"to": null});
		let a = json!({"to": "0x1234"});
		assert_eq!(compare(&e, &a, &CompareMode::Schema), MatchOutcome::Match);
	}
}
