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
