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

//! Debug rpc interface.

use ethereum_types::H256;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::types::{BlockNumberOrHash, Bytes};

/// Net rpc interface.
#[rpc(server)]
#[async_trait]
pub trait DebugApi {
	/// Returns an RLP-encoded header with the given number or hash.
	#[method(name = "debug_getRawHeader")]
	async fn raw_header(&self, number: BlockNumberOrHash) -> RpcResult<Option<Bytes>>;

	/// Returns an RLP-encoded block with the given number or hash.
	#[method(name = "debug_getRawBlock")]
	async fn raw_block(&self, number: BlockNumberOrHash) -> RpcResult<Option<Bytes>>;

	/// Returns a EIP-2718 binary-encoded transaction with the given hash.
	#[method(name = "debug_getRawTransaction")]
	async fn raw_transaction(&self, hash: H256) -> RpcResult<Option<Bytes>>;

	/// Returns an array of EIP-2718 binary-encoded receipts with the given number of hash.
	#[method(name = "debug_getRawReceipts")]
	async fn raw_receipts(&self, number: BlockNumberOrHash) -> RpcResult<Vec<Bytes>>;

	/// Returns an array of recent bad blocks that the client has seen on the network.
	#[method(name = "debug_getBadBlocks")]
	fn bad_blocks(&self, number: BlockNumberOrHash) -> RpcResult<Vec<()>>;
}
