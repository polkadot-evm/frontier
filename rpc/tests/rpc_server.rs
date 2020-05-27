// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Frontier.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use jsonrpc_core::IoHandler;

use frontier_rpc::{EthApiServer, EthRpcImpl};

mod eth_rpc_server {
	use super::*;

	#[test]
	fn protocol_version_0x54() {
		let mut handler = IoHandler::new();
		handler.extend_with(EthRpcImpl.to_delegate());

		let request = r#"{"jsonrpc": "2.0", "method": "eth_protocolVersion", "id": 1}"#;
		let response = r#"{"jsonrpc":"2.0","result":"0x54","id":1}"#;

		assert_eq!(
			handler.handle_request_sync(request).unwrap(),
			response.to_string()
		);
	}
}
