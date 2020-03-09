#[rpc]
pub trait EthereumApi {
	/// Returns protocol version.
	#[rpc(name = "net_version")]
	fn net_version(&self) -> Result<String>;

	/// Returns number of peers connected to node.
	#[rpc(name = "net_peerCount")]
	fn net_peer_count(&self) -> Result<String>;

	/// Returns true if client is actively listening for network connections.
	/// Otherwise false.
	#[rpc(name = "net_listening")]
	fn net_listening(&self) -> Result<bool>;
}
