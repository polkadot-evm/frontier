// This file is part of Tokfin.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use ethereum_types::U64;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

/// Net RPC interface.
#[rpc(client, server, namespace = "net")]
#[async_trait]
pub trait NetApi {
	/// Returns the network ID (e.g. 1 for mainnet, 5 for goerli).
	#[method(name = "version")]
	async fn version(&self) -> RpcResult<String>;

	/// Returns the number of connected peers.
	#[method(name = "peerCount")]
	async fn peer_count(&self) -> RpcResult<U64>;

	/// Returns an indication if the node is listening for network connections.
	#[method(name = "listening")]
	async fn listening(&self) -> RpcResult<bool>;
}
