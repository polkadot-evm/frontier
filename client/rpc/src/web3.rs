// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::{marker::PhantomData, sync::Arc};

use ethereum_types::H256;
use jsonrpc_core::Result;
use sp_api::{Core, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::keccak_256;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};

use fc_rpc_core::{types::Bytes, Web3Api as Web3ApiT};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::internal_err;

pub struct Web3Api<B, C> {
	client: Arc<C>,
	_marker: PhantomData<B>,
}

impl<B, C> Web3Api<B, C> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<B, C> Web3ApiT for Web3Api<B, C>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: HeaderBackend<B> + ProvideRuntimeApi<B> + Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
{
	fn client_version(&self) -> Result<String> {
		let hash = self.client.info().best_hash;
		let version = self
			.client
			.runtime_api()
			.version(&BlockId::Hash(hash))
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

	fn sha3(&self, input: Bytes) -> Result<H256> {
		Ok(H256::from(keccak_256(&input.into_vec())))
	}
}
