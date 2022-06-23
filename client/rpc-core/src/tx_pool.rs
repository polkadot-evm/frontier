// Copyright 2019-2022 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

use jsonrpsee::{core::RpcResult as Result, proc_macros::rpc};

use ethereum_types::U256;

pub use crate::types::{Get, Summary, TransactionContent, TransactionMap, TxPoolResult};

/// TxPool rpc interface.
#[rpc(server)]
pub trait TxPoolApi {
	#[method(name = "txpool_content")]
	fn content(&self) -> Result<TxPoolResult<TransactionMap<TransactionContent>>>;

	#[method(name = "txpool_inspect")]
	fn inspect(&self) -> Result<TxPoolResult<TransactionMap<Summary>>>;

	#[method(name = "txpool_status")]
	fn status(&self) -> Result<TxPoolResult<U256>>;
}
