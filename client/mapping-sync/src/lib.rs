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

use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use sp_blockchain::HeaderBackend;
use fp_consensus::ConsensusLog;

pub fn sync_block<Block: BlockT, C>(
	client: &C,
	backend: &fc_db::Backend<Block>,
	hash: Block::Hash,
) -> Result<(), String> where
	C: HeaderBackend<Block>,
{
	let header = client.header(BlockId::Hash(hash)).map_err(|e| format!("{:?}", e))?
		.ok_or("Header not found".to_string())?;
	let log = fc_consensus::find_frontier_log::<Block>(&header)?;
	let post_hashes = match log {
		ConsensusLog::PostHashes(post_hashes) => post_hashes,
		ConsensusLog::PreBlock(block) => fp_consensus::PostHashes::from_block(block),
		ConsensusLog::PostBlock(block) => fp_consensus::PostHashes::from_block(block),
	};

	let mapping_commitment = fc_db::MappingCommitment {
		block_hash: hash,
		ethereum_block_hash: post_hashes.block_hash,
		ethereum_transaction_hashes: post_hashes.transaction_hashes,
	};
	backend.mapping().write_hashes(mapping_commitment)?;

	Ok(())
}
