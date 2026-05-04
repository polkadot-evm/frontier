//! Compare an actual JSON-RPC response against the expected one, with an
//! allow-list for fields that are legitimately dynamic on a fresh dev chain
//! (block hashes, timestamps, etc.). The allow-list is intentionally small —
//! if it grows, that is a signal of real divergence rather than a fix.

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

pub fn compare(expected: &Value, actual: &Value, dynamic: &DynamicFields) -> MatchOutcome {
	compare_at(expected, actual, dynamic, &mut Vec::new())
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

	#[test]
	fn matches_identical_objects() {
		let v = json!({"a":1,"b":[1,2,3]});
		assert_eq!(compare(&v, &v, &DynamicFields::default()), MatchOutcome::Match);
	}

	#[test]
	fn detects_value_mismatch() {
		let e = json!({"a":1});
		let a = json!({"a":2});
		let out = compare(&e, &a, &DynamicFields::default());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.a"));
	}

	#[test]
	fn detects_missing_field() {
		let e = json!({"a":1,"b":2});
		let a = json!({"a":1});
		let out = compare(&e, &a, &DynamicFields::default());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.b"));
	}

	#[test]
	fn detects_unexpected_field() {
		let e = json!({"a":1});
		let a = json!({"a":1,"b":2});
		let out = compare(&e, &a, &DynamicFields::default());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.b"));
	}

	#[test]
	fn dynamic_fields_only_check_shape() {
		let e = json!({"hash":"0xaaaa","number":"0x1"});
		let a = json!({"hash":"0xbbbb","number":"0x1"});
		assert_eq!(
			compare(&e, &a, &DynamicFields::new(["hash"])),
			MatchOutcome::Match
		);
	}

	#[test]
	fn dynamic_fields_still_catch_shape_diffs() {
		let e = json!({"hash":"0xaaaa"});
		let a = json!({"hash":null});
		let out = compare(&e, &a, &DynamicFields::new(["hash"]));
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.hash"));
	}

	#[test]
	fn array_length_must_match() {
		let e = json!([1, 2, 3]);
		let a = json!([1, 2]);
		let out = compare(&e, &a, &DynamicFields::default());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$"));
	}

	#[test]
	fn nested_path_reported() {
		let e = json!({"result":{"items":[{"x":1}]}});
		let a = json!({"result":{"items":[{"x":2}]}});
		let out = compare(&e, &a, &DynamicFields::default());
		assert!(matches!(out, MatchOutcome::Mismatch { ref path, .. } if path == "$.result.items[0].x"));
	}
}
