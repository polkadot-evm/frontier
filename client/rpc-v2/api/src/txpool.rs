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

use ethereum_types::Address;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::types::txpool::{TxpoolContent, TxpoolContentFrom, TxpoolInspect, TxpoolStatus};

/// TxPool RPC interface.
#[rpc(client, server, namespace = "txpool")]
#[async_trait]
pub trait TxPoolApi {
	/// The content inspection property can be queried to list the exact details of all the
	/// transactions currently pending for inclusion in the next block(s), as well as the ones that
	/// are being scheduled for future execution only.
	///
	/// The result is an object with two fields pending and queued. Each of these fields are
	/// associative arrays, in which each entry maps an origin-address to a batch of scheduled
	/// transactions. These batches themselves are maps associating nonces with actual transactions.
	///
	/// Refer to [txpool_content](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-content).
	#[method(name = "content")]
	async fn content(&self) -> RpcResult<TxpoolContent>;

	/// Retrieves the transactions contained within the txpool, returning pending as well as queued
	/// transactions of this address, grouped by nonce.
	///
	/// Refer to [txpool_contentFrom](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-contentfrom).
	#[method(name = "contentFrom")]
	async fn content_from(&self, address: Address) -> RpcResult<TxpoolContentFrom>;

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
	/// Refer to [txpool_inspect](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-inspect).
	#[method(name = "inspect")]
	async fn inspect(&self) -> RpcResult<TxpoolInspect>;

	/// The status inspection property can be queried for the number of transactions currently
	/// pending for inclusion in the next block(s), as well as the ones that are being scheduled
	/// for future execution only.
	///
	/// The result is an object with two fields pending and queued, each of which is a counter
	/// representing the number of transactions in that particular state.
	///
	/// Refer to [txpool_status](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-status).
	#[method(name = "status")]
	async fn status(&self) -> RpcResult<TxpoolStatus>;
}
