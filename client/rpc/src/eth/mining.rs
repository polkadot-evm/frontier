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

use ethereum_types::{H256, H64, U256};
use jsonrpc_core::Result;

use fc_rpc_core::{types::*, EthMiningApi as EthMiningApiT};

pub struct EthMiningApi {
	is_authority: bool,
}

impl EthMiningApi {
	pub fn new(is_authority: bool) -> Self {
		Self { is_authority }
	}
}

impl EthMiningApiT for EthMiningApi {
	fn is_mining(&self) -> Result<bool> {
		Ok(self.is_authority)
	}

	fn hashrate(&self) -> Result<U256> {
		Ok(U256::zero())
	}

	fn work(&self) -> Result<Work> {
		Ok(Work::default())
	}

	fn submit_hashrate(&self, _: U256, _: H256) -> Result<bool> {
		Ok(false)
	}

	fn submit_work(&self, _: H64, _: H256, _: H256) -> Result<bool> {
		Ok(false)
	}
}
