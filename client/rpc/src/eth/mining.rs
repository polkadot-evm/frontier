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

use ethereum_types::{H256, H64, U256};
use jsonrpsee::core::RpcResult;
// Substrate
use sc_transaction_pool::ChainApi;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::types::*;

use crate::eth::Eth;

impl<B, C, P, CT, BE, A, CIDP, EC> Eth<B, C, P, CT, BE, A, CIDP, EC>
where
	B: BlockT,
	A: ChainApi<Block = B>,
{
	pub fn is_mining(&self) -> RpcResult<bool> {
		Ok(self.is_authority)
	}

	pub fn hashrate(&self) -> RpcResult<U256> {
		Ok(U256::zero())
	}

	pub fn work(&self) -> RpcResult<Work> {
		Ok(Work::default())
	}

	pub fn submit_hashrate(&self, _: U256, _: H256) -> RpcResult<bool> {
		Ok(false)
	}

	pub fn submit_work(&self, _: H64, _: H256, _: H256) -> RpcResult<bool> {
		Ok(false)
	}
}
