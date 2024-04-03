// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Net rpc interface.

use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::types::PeerCount;

/// Net rpc interface.
#[rpc(server)]
pub trait NetApi {
	/// Returns protocol version.
	#[method(name = "net_version")]
	fn version(&self) -> RpcResult<String>;

	/// Returns number of peers connected to node.
	#[method(name = "net_peerCount")]
	fn peer_count(&self) -> RpcResult<PeerCount>;

	/// Returns true if client is actively listening for network connections.
	/// Otherwise false.
	#[method(name = "net_listening")]
	fn is_listening(&self) -> RpcResult<bool>;
}
