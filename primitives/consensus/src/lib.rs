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
use sp_runtime::{
	ConsensusEngineId, generic::OpaqueDigestItemId,
	traits::{Block as BlockT, Header as HeaderT},
};
use sha3::{Digest, Keccak256};

pub const FRONTIER_ENGINE_ID: ConsensusEngineId = [b'f', b'r', b'o', b'n'];

#[derive(Clone, PartialEq, Eq)]
pub enum Log {
	Pre(PreLog),
	Post(PostLog),
}

impl Log {
	pub fn into_hashes(self) -> Hashes {
		match self {
			Log::Post(PostLog::Hashes(post_hashes)) => post_hashes,
			Log::Post(PostLog::Block(block)) => Hashes::from_block(block),
			Log::Pre(PreLog::Block(block)) => Hashes::from_block(block),
		}
	}
}

#[derive(Decode, Encode, Clone, PartialEq, Eq)]
pub enum PreLog {
	#[codec(index = "3")]
	Block(ethereum::Block),
}

#[derive(Decode, Encode, Clone, PartialEq, Eq)]
pub enum PostLog {
	#[codec(index = "1")]
	Hashes(Hashes),
	#[codec(index = "2")]
	Block(ethereum::Block),
}

#[derive(Decode, Encode, Clone, PartialEq, Eq)]
pub struct Hashes {
	/// Ethereum block hash.
	pub block_hash: H256,
	/// Transaction hashes of the Ethereum block.
	pub transaction_hashes: Vec<H256>,
}

impl Hashes {
	pub fn from_block(block: ethereum::Block) -> Self {
		let mut transaction_hashes = Vec::new();

		for t in &block.transactions {
			let transaction_hash = H256::from_slice(
				Keccak256::digest(&rlp::encode(t)).as_slice()
			);
			transaction_hashes.push(transaction_hash);
		}

		let block_hash = block.header.hash();

		Hashes { transaction_hashes, block_hash }
	}
}

#[derive(Clone, Debug)]
pub enum FindLogError {
	NotFound,
	MultipleLogs,
}

pub fn find_pre_log<Block: BlockT>(
	header: &Block::Header,
) -> Result<PreLog, FindLogError> {
	let mut found = None;

	for log in header.digest().logs() {
		let log = log.try_to::<PreLog>(OpaqueDigestItemId::PreRuntime(&FRONTIER_ENGINE_ID));
		match (log, found.is_some()) {
			(Some(_), true) => return Err(FindLogError::MultipleLogs),
			(Some(log), false) => found = Some(log),
			(None, _) => (),
		}
	}

	found.ok_or(FindLogError::NotFound)
}

pub fn find_post_log<Block: BlockT>(
	header: &Block::Header,
) -> Result<PostLog, FindLogError> {
	let mut found = None;

	for log in header.digest().logs() {
		let log = log.try_to::<PostLog>(OpaqueDigestItemId::Consensus(&FRONTIER_ENGINE_ID));
		match (log, found.is_some()) {
			(Some(_), true) => return Err(FindLogError::MultipleLogs),
			(Some(log), false) => found = Some(log),
			(None, _) => (),
		}
	}

	found.ok_or(FindLogError::NotFound)
}

pub fn find_log<Block: BlockT>(
	header: &Block::Header,
) -> Result<Log, FindLogError> {
	let mut found = None;

	for log in header.digest().logs() {
		let pre_log = log.try_to::<PreLog>(OpaqueDigestItemId::PreRuntime(&FRONTIER_ENGINE_ID));
		match (pre_log, found.is_some()) {
			(Some(_), true) => return Err(FindLogError::MultipleLogs),
			(Some(pre_log), false) => found = Some(Log::Pre(pre_log)),
			(None, _) => (),
		}

		let post_log = log.try_to::<PostLog>(OpaqueDigestItemId::Consensus(&FRONTIER_ENGINE_ID));
		match (post_log, found.is_some()) {
			(Some(_), true) => return Err(FindLogError::MultipleLogs),
			(Some(post_log), false) => found = Some(Log::Post(post_log)),
			(None, _) => (),
		}
	}

	found.ok_or(FindLogError::NotFound)
}

pub fn ensure_log<Block: BlockT>(
	header: &Block::Header,
) -> Result<(), FindLogError> {
	let mut found = false;

	for log in header.digest().logs() {
		let pre_log = log.try_to::<PreLog>(OpaqueDigestItemId::PreRuntime(&FRONTIER_ENGINE_ID));
		match (pre_log, found) {
			(Some(_), true) => return Err(FindLogError::MultipleLogs),
			(Some(_), false) => found = true,
			(None, _) => (),
		}

		let post_log = log.try_to::<PostLog>(OpaqueDigestItemId::Consensus(&FRONTIER_ENGINE_ID));
		match (post_log, found) {
			(Some(_), true) => return Err(FindLogError::MultipleLogs),
			(Some(_), false) => found = true,
			(None, _) => (),
		}
	}

	if found {
		Ok(())
	} else {
		Err(FindLogError::NotFound)
	}
}
