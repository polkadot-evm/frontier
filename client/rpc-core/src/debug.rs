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

	/// Returns an array of EIP-2718 binary-encoded receipts with the given number or hash.
	#[method(name = "debug_getRawReceipts")]
	async fn raw_receipts(&self, number: BlockNumberOrHash) -> RpcResult<Vec<Bytes>>;

	/// Returns an array of recent bad blocks that the client has seen on the network.
	#[method(name = "debug_getBadBlocks")]
	fn bad_blocks(&self, number: BlockNumberOrHash) -> RpcResult<Vec<()>>;
}
