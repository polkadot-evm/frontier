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

pub use worker::{MappingSyncWorker, SyncStrategy};

use fp_consensus::FindLogError;
use fp_rpc::EthereumRuntimeRPCApi;
use sc_client_api::BlockOf;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT, Zero},
};

pub fn sync_block<Block: BlockT>(
	backend: &fc_db::Backend<Block>,
	header: &Block::Header,
) -> Result<(), String> {
	match fp_consensus::find_log(header.digest()) {
		Ok(log) => {
			let post_hashes = log.into_hashes();

			let mapping_commitment = fc_db::MappingCommitment {
				block_hash: header.hash(),
				ethereum_block_hash: post_hashes.block_hash,
				ethereum_transaction_hashes: post_hashes.transaction_hashes,
			};
			backend.mapping().write_hashes(mapping_commitment)?;

			Ok(())
		}
		Err(FindLogError::NotFound) => {
			backend.mapping().write_none(header.hash())?;

			Ok(())
		}
		Err(FindLogError::MultipleLogs) => Err("Multiple logs found".to_string()),
	}
}

pub fn sync_genesis_block<Block: BlockT, C>(
	client: &C,
	backend: &fc_db::Backend<Block>,
	header: &Block::Header,
) -> Result<(), String>
where
	C: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<Block>,
{
	let id = BlockId::Hash(header.hash());

	let has_api = client
		.runtime_api()
		.has_api::<dyn EthereumRuntimeRPCApi<Block>>(&id)
		.map_err(|e| format!("{:?}", e))?;

	if has_api {
		let block = client
			.runtime_api()
			.current_block(&id)
			.map_err(|e| format!("{:?}", e))?;
		let block_hash = block
			.ok_or("Ethereum genesis block not found".to_string())?
			.header
			.hash();
		let mapping_commitment = fc_db::MappingCommitment::<Block> {
			block_hash: header.hash(),
			ethereum_block_hash: block_hash,
			ethereum_transaction_hashes: Vec::new(),
		};
		backend.mapping().write_hashes(mapping_commitment)?;
	} else {
		backend.mapping().write_none(header.hash())?;
	}

	Ok(())
}

pub fn sync_one_block<Block: BlockT, C, B>(
	client: &C,
	substrate_backend: &B,
	frontier_backend: &fc_db::Backend<Block>,
	strategy: SyncStrategy,
) -> Result<bool, String>
where
	C: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<Block>,
	B: sp_blockchain::HeaderBackend<Block> + sp_blockchain::Backend<Block>,
{
	let mut current_syncing_tips = frontier_backend.meta().current_syncing_tips()?;

	if current_syncing_tips.is_empty() {
		let mut leaves = substrate_backend.leaves().map_err(|e| format!("{:?}", e))?;
		if leaves.is_empty() {
			return Ok(false);
		}

		current_syncing_tips.append(&mut leaves);
	}

	let mut operating_tip = None;

	while let Some(checking_tip) = current_syncing_tips.pop() {
		if !frontier_backend
			.mapping()
			.is_synced(&checking_tip)
			.map_err(|e| format!("{:?}", e))?
		{
			operating_tip = Some(checking_tip);
			break;
		}
	}

	let operating_tip = match operating_tip {
		Some(operating_tip) => operating_tip,
		None => {
			frontier_backend
				.meta()
				.write_current_syncing_tips(current_syncing_tips)?;
			return Ok(false);
		}
	};

	let operating_header = substrate_backend
		.header(BlockId::Hash(operating_tip))
		.map_err(|e| format!("{:?}", e))?
		.ok_or("Header not found".to_string())?;

	if operating_header.number() == &Zero::zero() {
		sync_genesis_block(client, frontier_backend, &operating_header)?;

		frontier_backend
			.meta()
			.write_current_syncing_tips(current_syncing_tips)?;
		Ok(true)
	} else {
		if SyncStrategy::Parachain == strategy
			&& operating_header.number() > &client.info().best_number
		{
			return Ok(false);
		}
		sync_block(frontier_backend, &operating_header)?;

		current_syncing_tips.push(*operating_header.parent_hash());
		frontier_backend
			.meta()
			.write_current_syncing_tips(current_syncing_tips)?;
		Ok(true)
	}
}

pub fn sync_blocks<Block: BlockT, C, B>(
	client: &C,
	substrate_backend: &B,
	frontier_backend: &fc_db::Backend<Block>,
	limit: usize,
	strategy: SyncStrategy,
) -> Result<bool, String>
where
	C: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<Block>,
	B: sp_blockchain::HeaderBackend<Block> + sp_blockchain::Backend<Block>,
{
	let mut synced_any = false;

	for _ in 0..limit {
		synced_any =
			synced_any || sync_one_block(client, substrate_backend, frontier_backend, strategy)?;
	}

	Ok(synced_any)
}
