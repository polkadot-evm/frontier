//! Parser for the rpctestgen `.io` vector format.

use serde_json::Value;
use thiserror::Error;

/// One request/response pair from a vector file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
	pub request: Value,
	pub response: Value,
}

/// A parsed vector file: any number of request/response pairs plus directives
/// gathered from `//` comment lines (e.g. `speconly`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vector {
	pub exchanges: Vec<Exchange>,
	pub speconly: bool,
}

#[derive(Debug, Error)]
pub enum ParseError {
	#[error("line {line}: expected '>>' request, found '<<' response")]
	UnexpectedResponse { line: usize },
	#[error("line {line}: expected '<<' response after '>>' request")]
	MissingResponse { line: usize },
	#[error("line {line}: invalid JSON: {source}")]
	InvalidJson {
		line: usize,
		#[source]
		source: serde_json::Error,
	},
	#[error("line {line}: unrecognized line prefix")]
	UnknownPrefix { line: usize },
}

pub fn parse(input: &str) -> Result<Vector, ParseError> {
	let mut vector = Vector::default();
	let mut pending_request: Option<(usize, Value)> = None;

	for (idx, raw) in input.lines().enumerate() {
		let line_no = idx + 1;
		let line = raw.trim();
		if line.is_empty() {
			continue;
		}

		if let Some(rest) = line.strip_prefix("//") {
			let directive = rest.trim();
			if directive.starts_with("speconly") {
				vector.speconly = true;
			}
			continue;
		}

		if let Some(rest) = line.strip_prefix(">>") {
			if let Some((req_line, _)) = pending_request {
				return Err(ParseError::MissingResponse { line: req_line });
			}
			let value = parse_json(rest, line_no)?;
			pending_request = Some((line_no, value));
		} else if let Some(rest) = line.strip_prefix("<<") {
			let Some((_, request)) = pending_request.take() else {
				return Err(ParseError::UnexpectedResponse { line: line_no });
			};
			let response = parse_json(rest, line_no)?;
			vector.exchanges.push(Exchange { request, response });
		} else {
			return Err(ParseError::UnknownPrefix { line: line_no });
		}
	}

	if let Some((line, _)) = pending_request {
		return Err(ParseError::MissingResponse { line });
	}

	Ok(vector)
}

fn parse_json(rest: &str, line: usize) -> Result<Value, ParseError> {
	serde_json::from_str(rest.trim()).map_err(|source| ParseError::InvalidJson { line, source })
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn parses_simple_pair() {
		let input = r#"
// retrieves the client's current block number
>> {"jsonrpc":"2.0","id":1,"method":"eth_blockNumber"}
<< {"jsonrpc":"2.0","id":1,"result":"0x2d"}
"#;
		let v = parse(input).unwrap();
		assert!(!v.speconly);
		assert_eq!(v.exchanges.len(), 1);
		assert_eq!(
			v.exchanges[0].request,
			json!({"jsonrpc":"2.0","id":1,"method":"eth_blockNumber"})
		);
		assert_eq!(
			v.exchanges[0].response,
			json!({"jsonrpc":"2.0","id":1,"result":"0x2d"})
		);
	}

	#[test]
	fn parses_multiple_pairs() {
		let input = r#"
>> {"jsonrpc":"2.0","id":1,"method":"a"}
<< {"jsonrpc":"2.0","id":1,"result":"0x1"}
>> {"jsonrpc":"2.0","id":2,"method":"b"}
<< {"jsonrpc":"2.0","id":2,"result":"0x2"}
"#;
		let v = parse(input).unwrap();
		assert_eq!(v.exchanges.len(), 2);
		assert_eq!(v.exchanges[1].request["id"], 2);
	}

	#[test]
	fn detects_speconly_directive() {
		let input = r#"
// speconly: client response is only checked for schema validity.
>> {"jsonrpc":"2.0","id":1,"method":"a"}
<< {"jsonrpc":"2.0","id":1,"result":"0x1"}
"#;
		assert!(parse(input).unwrap().speconly);
	}

	#[test]
	fn rejects_response_without_request() {
		let input = "<< {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}\n";
		assert!(matches!(
			parse(input),
			Err(ParseError::UnexpectedResponse { line: 1 })
		));
	}

	#[test]
	fn rejects_request_without_response() {
		let input = ">> {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"a\"}\n";
		assert!(matches!(
			parse(input),
			Err(ParseError::MissingResponse { line: 1 })
		));
	}

	#[test]
	fn rejects_two_consecutive_requests() {
		let input = ">> {\"id\":1}\n>> {\"id\":2}\n";
		assert!(matches!(
			parse(input),
			Err(ParseError::MissingResponse { line: 1 })
		));
	}

	#[test]
	fn rejects_unknown_prefix() {
		let input = "?? {\"id\":1}\n";
		assert!(matches!(
			parse(input),
			Err(ParseError::UnknownPrefix { line: 1 })
		));
	}

	#[test]
	fn rejects_invalid_json() {
		let input = ">> not json\n";
		assert!(matches!(
			parse(input),
			Err(ParseError::InvalidJson { line: 1, .. })
		));
	}

	#[test]
	fn ignores_blank_lines_and_whitespace() {
		let input = "\n\n  // hi\n>>    {\"id\":1}\n<<    {\"id\":1,\"r\":1}\n\n";
		let v = parse(input).unwrap();
		assert_eq!(v.exchanges.len(), 1);
	}
}
