// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2022 Parity Technologies (UK) Ltd.
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

//! tx pool rpc interface

use ethereum_types::U256;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::types::*;

/// TxPool rpc interface
#[rpc(server)]
pub trait TxPoolApi {
	/// The content inspection property can be queried to list the exact details of all the
	/// transactions currently pending for inclusion in the next block(s), as well as the ones that
	/// are being scheduled for future execution only.
	///
	/// The result is an object with two fields pending and queued. Each of these fields are
	/// associative arrays, in which each entry maps an origin-address to a batch of scheduled
	/// transactions. These batches themselves are maps associating nonces with actual transactions.
	///
	/// For details, see [txpool_content](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-content)
	#[method(name = "txpool_content")]
	fn content(&self) -> RpcResult<TxPoolResult<TransactionMap<TxPoolTransaction>>>;

	/// The inspect inspection property can be queried to list a textual summary of all the
	/// transactions currently pending for inclusion in the next block(s), as well as the ones that
	/// are being scheduled for future execution only. This is a method specifically tailored to
	/// developers to quickly see the transactions in the pool and find any potential issues.
	///
	/// The result is an object with two fields pending and queued. Each of these fields are
	/// associative arrays, in which each entry maps an origin-address to a batch of scheduled
	/// transactions. These batches themselves are maps associating nonces with transactions
	/// summary strings.
	///
	/// For details, see [txpool_inspect](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-content)
	#[method(name = "txpool_inspect")]
	fn inspect(&self) -> RpcResult<TxPoolResult<TransactionMap<Summary>>>;

	/// The status inspection property can be queried for the number of transactions currently
	/// pending for inclusion in the next block(s), as well as the ones that are being scheduled
	/// for future execution only.
	///
	/// The result is an object with two fields pending and queued, each of which is a counter
	/// representing the number of transactions in that particular state.
	///
	/// For details, see [txpool_status](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-status)
	#[method(name = "txpool_status")]
	fn status(&self) -> RpcResult<TxPoolResult<U256>>;
}
