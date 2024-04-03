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

use std::{marker::PhantomData, sync::Arc};

use ethereum_types::H256;
use jsonrpsee::core::RpcResult;
// Substrate
use sp_api::{Core, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::keccak_256;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::{types::Bytes, Web3ApiServer};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::internal_err;

/// Web3 API implementation.
pub struct Web3<B, C> {
	client: Arc<C>,
	_marker: PhantomData<B>,
}

impl<B, C> Web3<B, C> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<B, C> Web3ApiServer for Web3<B, C>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + 'static,
{
	fn client_version(&self) -> RpcResult<String> {
		let hash = self.client.info().best_hash;
		let version = self
			.client
			.runtime_api()
			.version(hash)
			.map_err(|err| internal_err(format!("fetch runtime version failed: {:?}", err)))?;
		Ok(format!(
			"{spec_name}/v{spec_version}.{impl_version}/{pkg_name}-{pkg_version}",
			spec_name = version.spec_name,
			spec_version = version.spec_version,
			impl_version = version.impl_version,
			pkg_name = env!("CARGO_PKG_NAME"),
			pkg_version = env!("CARGO_PKG_VERSION")
		))
	}

	fn sha3(&self, input: Bytes) -> RpcResult<H256> {
		Ok(H256::from(keccak_256(&input.into_vec())))
	}
}
