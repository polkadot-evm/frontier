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

use std::sync::Arc;

use jsonrpsee::core::RpcResult;
// Substrate
use sc_network::{NetworkPeers, NetworkService};
use sc_network_common::ExHashT;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::{types::PeerCount, NetApiServer};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::internal_err;

/// Net API implementation.
pub struct Net<B: BlockT, C, H: ExHashT> {
	client: Arc<C>,
	network: Arc<NetworkService<B, H>>,
	peer_count_as_hex: bool,
}

impl<B: BlockT, C, H: ExHashT> Net<B, C, H> {
	pub fn new(
		client: Arc<C>,
		network: Arc<NetworkService<B, H>>,
		peer_count_as_hex: bool,
	) -> Self {
		Self {
			client,
			network,
			peer_count_as_hex,
		}
	}
}

impl<B, C, H: ExHashT> NetApiServer for Net<B, C, H>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + 'static,
{
	fn version(&self) -> RpcResult<String> {
		let hash = self.client.info().best_hash;
		Ok(self
			.client
			.runtime_api()
			.chain_id(hash)
			.map_err(|_| internal_err("fetch runtime chain id failed"))?
			.to_string())
	}

	fn peer_count(&self) -> RpcResult<PeerCount> {
		let peer_count = self.network.sync_num_connected();
		Ok(match self.peer_count_as_hex {
			true => PeerCount::String(format!("0x{:x}", peer_count)),
			false => PeerCount::U32(peer_count as u32),
		})
	}

	fn is_listening(&self) -> RpcResult<bool> {
		Ok(true)
	}
}
