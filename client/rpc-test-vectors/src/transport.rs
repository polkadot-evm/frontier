//! Synchronous HTTP transport over plain JSON-RPC. Sends the literal request
//! JSON and returns the literal response JSON — no typed (de)serialization
//! between the wire and the comparator.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use crate::runner::Transport;

pub struct HttpTransport {
	url: String,
	agent: ureq::Agent,
}

impl HttpTransport {
	pub fn new(url: impl Into<String>) -> Self {
		let agent = ureq::AgentBuilder::new()
			.timeout_connect(Duration::from_secs(2))
			.timeout(Duration::from_secs(10))
			.build();
		Self {
			url: url.into(),
			agent,
		}
	}
}

impl Transport for HttpTransport {
	fn send(&self, request: &Value) -> Result<Value, String> {
		let body = request.to_string();
		let response = self
			.agent
			.post(&self.url)
			.set("content-type", "application/json")
			.send_string(&body)
			.map_err(|e| format!("HTTP error: {e}"))?;

		let mut buf = String::new();
		response
			.into_reader()
			.take(16 * 1024 * 1024)
			.read_to_string(&mut buf)
			.map_err(|e| format!("read error: {e}"))?;
		serde_json::from_str(&buf).map_err(|e| format!("response is not JSON: {e}; body={buf}"))
	}
}
