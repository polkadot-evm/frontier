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

mod worker;

pub use worker::MappingSyncWorker;

use sp_runtime::{generic::BlockId, traits::{Block as BlockT, Header as HeaderT, Zero}};
use sp_api::ProvideRuntimeApi;
use sc_client_api::BlockOf;
use sp_blockchain::HeaderBackend;
use fp_consensus::ConsensusLog;
use fp_rpc::EthereumRuntimeRPCApi;

pub fn sync_block<Block: BlockT>(
	backend: &fc_db::Backend<Block>,
	header: &Block::Header,
) -> Result<(), String> {
	let log = fc_consensus::find_frontier_log::<Block>(&header)?;
	let post_hashes = match log {
		ConsensusLog::PostHashes(post_hashes) => post_hashes,
		ConsensusLog::PreBlock(block) => fp_consensus::PostHashes::from_block(block),
		ConsensusLog::PostBlock(block) => fp_consensus::PostHashes::from_block(block),
	};

	let mapping_commitment = fc_db::MappingCommitment {
		block_hash: header.hash(),
		ethereum_block_hash: post_hashes.block_hash,
		ethereum_transaction_hashes: post_hashes.transaction_hashes,
	};
	backend.mapping().write_hashes(mapping_commitment)?;

	Ok(())
}

pub fn sync_genesis_block<Block: BlockT, C>(
	client: &C,
	backend: &fc_db::Backend<Block>,
	header: &Block::Header,
) -> Result<(), String> where
	C: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<Block>,
{
	let id = BlockId::Hash(header.hash());

	let block = client.runtime_api().current_block(&id)
		.map_err(|e| format!("{:?}", e))?;
	let block_hash = block.ok_or("Ethereum genesis block not found".to_string())?.header.hash();
	let mapping_commitment = fc_db::MappingCommitment::<Block> {
		block_hash: header.hash(),
		ethereum_block_hash: block_hash,
		ethereum_transaction_hashes: Vec::new(),
	};
	backend.mapping().write_hashes(mapping_commitment)?;

	Ok(())
}

pub fn sync_one_level<Block: BlockT, C, B>(
	client: &C,
	substrate_backend: &B,
	frontier_backend: &fc_db::Backend<Block>,
) -> Result<bool, String> where
	C: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<Block>,
	B: sp_blockchain::HeaderBackend<Block> + sp_blockchain::Backend<Block>,
{
	let mut current_syncing_tips = frontier_backend.meta().current_syncing_tips()?;

	if current_syncing_tips.len() == 0 {
		// Sync genesis block.

		let header = substrate_backend.header(BlockId::Number(Zero::zero()))
			.map_err(|e| format!("{:?}", e))?
			.ok_or("Genesis header not found".to_string())?;
		sync_genesis_block(client, frontier_backend, &header)?;

		current_syncing_tips.push(header.hash());
		frontier_backend.meta().write_current_syncing_tips(current_syncing_tips)?;

		Ok(true)
	} else {
		let mut syncing_tip_and_children = None;

		for tip in &current_syncing_tips {
			let children = substrate_backend.children(*tip)
				.map_err(|e| format!("{:?}", e))?;

			if children.len() > 0 {
				syncing_tip_and_children = Some((*tip, children));
				break
			}
		}

		if let Some((syncing_tip, children)) = syncing_tip_and_children {
			current_syncing_tips.retain(|tip| tip != &syncing_tip);

			for child in children {
				let header = substrate_backend.header(BlockId::Hash(child))
					.map_err(|e| format!("{:?}", e))?
					.ok_or("Header not found".to_string())?;

				sync_block(frontier_backend, &header)?;
				current_syncing_tips.push(child);
			}
			frontier_backend.meta().write_current_syncing_tips(current_syncing_tips)?;

			Ok(true)
		} else {
			Ok(false)
		}
	}
}

pub fn sync_blocks<Block: BlockT, C, B>(
	client: &C,
	substrate_backend: &B,
	frontier_backend: &fc_db::Backend<Block>,
	limit: usize,
) -> Result<bool, String> where
	C: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<Block>,
	B: sp_blockchain::HeaderBackend<Block> + sp_blockchain::Backend<Block>,
{
	let mut synced_any = false;

	for _ in 0..limit {
		synced_any = synced_any || sync_one_level(client, substrate_backend, frontier_backend)?;
	}

	Ok(synced_any)
}
