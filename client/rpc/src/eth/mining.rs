// This file is part of Frontier.

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

use ethereum_types::{H256, H64, U256};
use jsonrpsee::core::RpcResult;
// Substrate
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::types::*;

use crate::eth::Eth;

impl<B, C, P, CT, BE, CIDP, EC> Eth<B, C, P, CT, BE, CIDP, EC>
where
	B: BlockT,
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
