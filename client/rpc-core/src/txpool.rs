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
	/// For details, see [txpool_content (geth)](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-content)
	/// or [txpool_content (nethermind)](https://docs.nethermind.io/nethermind/ethereum-client/json-rpc/txpool#txpool_content).
	#[method(name = "txpool_content")]
	fn content(&self) -> RpcResult<TxPoolResult<TransactionMap<Transaction>>>;

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
	/// For details, see [txpool_inspect (geth)](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-inspect)
	/// or [txpool_inspect (nethermind)](https://docs.nethermind.io/nethermind/ethereum-client/json-rpc/txpool#txpool_inspect).
	#[method(name = "txpool_inspect")]
	fn inspect(&self) -> RpcResult<TxPoolResult<TransactionMap<Summary>>>;

	/// The status inspection property can be queried for the number of transactions currently
	/// pending for inclusion in the next block(s), as well as the ones that are being scheduled
	/// for future execution only.
	///
	/// The result is an object with two fields pending and queued, each of which is a counter
	/// representing the number of transactions in that particular state.
	///
	/// For details, see [txpool_status (geth)](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool#txpool-status)
	/// or [txpool_status (nethermind)](https://docs.nethermind.io/nethermind/ethereum-client/json-rpc/txpool#txpool_status).
	#[method(name = "txpool_status")]
	fn status(&self) -> RpcResult<TxPoolResult<U256>>;
}
