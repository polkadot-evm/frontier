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

//! Web3 rpc interface.

use ethereum_types::H256;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::types::Bytes;

/// Web3 rpc interface.
#[rpc(server)]
pub trait Web3Api {
	/// Returns current client version.
	#[method(name = "web3_clientVersion")]
	fn client_version(&self) -> RpcResult<String>;

	/// Returns sha3 of the given data
	#[method(name = "web3_sha3")]
	fn sha3(&self, input: Bytes) -> RpcResult<H256>;
}
