// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
//
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

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Encode, Decode};
use sp_std::vec::Vec;
use sp_core::H256;
use sp_runtime::ConsensusEngineId;

pub const FRONTIER_ENGINE_ID: ConsensusEngineId = [b'f', b'r', b'o', b'n'];

#[derive(Decode, Encode, Clone, PartialEq, Eq)]
pub enum ConsensusLog {
	#[codec(index = "1")]
	EndBlock {
		/// Ethereum block hash.
		block_hash: H256,
		/// Transaction hashes of the Ethereum block.
		transaction_hashes: Vec<H256>,
	},
}
