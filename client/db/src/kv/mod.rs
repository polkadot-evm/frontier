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

mod parity_db_adapter;
mod upgrade;
mod utils;

use std::{
	marker::PhantomData,
	path::{Path, PathBuf},
	sync::Arc,
};

use parking_lot::Mutex;
use scale_codec::{Decode, Encode};
// Substrate
pub use sc_client_db::DatabaseSource;
use sp_blockchain::HeaderBackend;
use sp_core::{H160, H256};
pub use sp_database::Database;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};
// Frontier
use fc_api::{FilteredLog, TransactionMetadata};
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA_CACHE};

const DB_HASH_LEN: usize = 32;
/// Hash type that this backend uses for the database.
pub type DbHash = [u8; DB_HASH_LEN];

/// Maximum number of blocks to walk back when searching for an indexed canonical block.
/// This limits the search depth when the cached `LATEST_CANONICAL_INDEXED_BLOCK` is stale
/// (e.g., after a reorg or if it points to an unindexed block).
const MAX_WALKBACK_DEPTH: u64 = 16;

/// Database settings.
pub struct DatabaseSettings {
	/// Where to find the database.
	pub source: DatabaseSource,
}

pub(crate) mod columns {
	pub const NUM_COLUMNS: u32 = 5;

	pub const META: u32 = 0;
	pub const BLOCK_MAPPING: u32 = 1;
	pub const TRANSACTION_MAPPING: u32 = 2;
	pub const SYNCED_MAPPING: u32 = 3;
	pub const BLOCK_NUMBER_MAPPING: u32 = 4;
}

pub mod static_keys {
	pub const CURRENT_SYNCING_TIPS: &[u8] = b"CURRENT_SYNCING_TIPS";
	pub const LATEST_CANONICAL_INDEXED_BLOCK: &[u8] = b"LATEST_CANONICAL_INDEXED_BLOCK";
}

#[derive(Clone)]
pub struct Backend<Block, C> {
	client: Arc<C>,
	meta: Arc<MetaDb<Block>>,
	mapping: Arc<MappingDb<Block>>,
	log_indexer: LogIndexerBackend<Block>,
}

#[async_trait::async_trait]
impl<Block: BlockT, C: HeaderBackend<Block>> fc_api::Backend<Block> for Backend<Block, C> {
	async fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String> {
		self.mapping().block_hash(ethereum_block_hash)
	}

	async fn block_hash_by_number(&self, block_number: u64) -> Result<Option<H256>, String> {
		self.mapping().block_hash_by_number(block_number)
	}

	async fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String> {
		self.mapping()
			.transaction_metadata(ethereum_transaction_hash)
	}

	fn log_indexer(&self) -> &dyn fc_api::LogIndexerBackend<Block> {
		&self.log_indexer
	}

	async fn first_block_hash(&self) -> Result<Block::Hash, String> {
		Ok(self.client.info().genesis_hash)
	}

	async fn latest_block_hash(&self) -> Result<Block::Hash, String> {
		// Return the latest block hash that is both indexed AND on the canonical chain.
		// The canonical indexed block is tracked by mapping-sync when blocks are synced.
		//
		// Note: During initial sync or after restart while mapping-sync catches up,
		// this returns the genesis block hash. This is consistent with Geth's behavior
		// where eth_getBlockByNumber("latest") returns block 0 during initial sync.
		// Users can check sync status via eth_syncing to determine if the node is
		// still catching up.
		let best_number: u64 = self.client.info().best_number.unique_saturated_into();
		let (block_number, from_cached_meta) = match self.mapping.latest_canonical_indexed_block_number()? {
			Some(n) => (n, true),
			None => (best_number, false),
		};

		if let Some(canonical_hash) = self.indexed_canonical_hash_at(block_number)? {
			if !from_cached_meta {
				self.mapping.set_latest_canonical_indexed_block(block_number)?;
			}
			return Ok(canonical_hash);
		}

		// Cached canonical block is stale (reorg happened), or meta key was absent
		// and best block is not indexed yet. Walk back to the latest indexed
		// canonical block and persist the recovered pointer.
		if let Some((recovered_number, recovered_hash)) =
			self.find_latest_indexed_canonical_block(block_number.saturating_sub(1))?
		{
			self.mapping
				.set_latest_canonical_indexed_block(recovered_number)?;
			return Ok(recovered_hash);
		}

		Ok(self.client.info().genesis_hash)
	}
}

#[derive(Clone, Default)]
pub struct LogIndexerBackend<Block>(PhantomData<Block>);

#[async_trait::async_trait]
impl<Block: BlockT> fc_api::LogIndexerBackend<Block> for LogIndexerBackend<Block> {
	fn is_indexed(&self) -> bool {
		false
	}

	async fn filter_logs(
		&self,
		_from_block: u64,
		_to_block: u64,
		_addresses: Vec<H160>,
		_topics: Vec<Vec<H256>>,
	) -> Result<Vec<FilteredLog<Block>>, String> {
		Err("KeyValue db does not index logs".into())
	}
}

/// Returns the frontier database directory.
pub fn frontier_database_dir(db_config_dir: &Path, db_path: &str) -> PathBuf {
	db_config_dir.join("frontier").join(db_path)
}

impl<Block: BlockT, C: HeaderBackend<Block>> Backend<Block, C> {
	pub fn open(
		client: Arc<C>,
		database: &DatabaseSource,
		db_config_dir: &Path,
	) -> Result<Self, String> {
		Self::new(
			client,
			&DatabaseSettings {
				source: match database {
					DatabaseSource::Auto { .. } => DatabaseSource::Auto {
						rocksdb_path: frontier_database_dir(db_config_dir, "db"),
						paritydb_path: frontier_database_dir(db_config_dir, "paritydb"),
						cache_size: 0,
					},
					#[cfg(feature = "rocksdb")]
					DatabaseSource::RocksDb { .. } => DatabaseSource::RocksDb {
						path: frontier_database_dir(db_config_dir, "db"),
						cache_size: 0,
					},
					DatabaseSource::ParityDb { .. } => DatabaseSource::ParityDb {
						path: frontier_database_dir(db_config_dir, "paritydb"),
					},
					_ => {
						return Err(
							"Supported db sources: `auto` | `rocksdb` | `paritydb`".to_string()
						)
					}
				},
			},
		)
	}

	pub fn new(client: Arc<C>, config: &DatabaseSettings) -> Result<Self, String> {
		let db = utils::open_database::<Block, C>(client.clone(), config)?;

		Ok(Self {
			client,
			mapping: Arc::new(MappingDb {
				db: db.clone(),
				write_lock: Arc::new(Mutex::new(())),
				_marker: PhantomData,
			}),
			meta: Arc::new(MetaDb {
				db: db.clone(),
				_marker: PhantomData,
			}),
			log_indexer: LogIndexerBackend(PhantomData),
		})
	}

	pub fn mapping(&self) -> &Arc<MappingDb<Block>> {
		&self.mapping
	}

	pub fn meta(&self) -> &Arc<MetaDb<Block>> {
		&self.meta
	}

	/// Returns the canonical hash at `block_number` if it is indexed.
	fn indexed_canonical_hash_at(&self, block_number: u64) -> Result<Option<Block::Hash>, String> {
		let Some(eth_hash) = self.mapping.block_hash_by_number(block_number)? else {
			return Ok(None);
		};

		let Some(substrate_hashes) = self.mapping.block_hash(&eth_hash)? else {
			return Ok(None);
		};

		let Some(canonical_hash) = self
			.client
			.hash(block_number.unique_saturated_into())
			.map_err(|e| format!("{e:?}"))?
		else {
			return Ok(None);
		};

		if substrate_hashes.contains(&canonical_hash) {
			return Ok(Some(canonical_hash));
		}

		Ok(None)
	}

	/// Finds the latest indexed block that is on the canonical chain by walking
	/// backwards from `start_block`. Returns `None` if no indexed canonical block
	/// is found within `MAX_WALKBACK_DEPTH` blocks.
	fn find_latest_indexed_canonical_block(
		&self,
		start_block: u64,
	) -> Result<Option<(u64, Block::Hash)>, String> {
		let min_block = start_block.saturating_sub(MAX_WALKBACK_DEPTH);
		for block_number in (min_block..=start_block).rev() {
			if let Some(canonical_hash) = self.indexed_canonical_hash_at(block_number)? {
				return Ok(Some((block_number, canonical_hash)));
			}
		}

		Ok(None)
	}
}

pub struct MetaDb<Block> {
	db: Arc<dyn Database<DbHash>>,
	_marker: PhantomData<Block>,
}

impl<Block: BlockT> MetaDb<Block> {
	pub fn current_syncing_tips(&self) -> Result<Vec<Block::Hash>, String> {
		match self
			.db
			.get(columns::META, static_keys::CURRENT_SYNCING_TIPS)
		{
			Some(raw) => Ok(Vec::<Block::Hash>::decode(&mut &raw[..]).map_err(|e| e.to_string())?),
			None => Ok(Vec::new()),
		}
	}

	pub fn write_current_syncing_tips(&self, tips: Vec<Block::Hash>) -> Result<(), String> {
		let mut transaction = sp_database::Transaction::new();

		transaction.set(
			columns::META,
			static_keys::CURRENT_SYNCING_TIPS,
			&tips.encode(),
		);

		self.db.commit(transaction).map_err(|e| e.to_string())?;

		Ok(())
	}

	pub fn ethereum_schema(&self) -> Result<Option<Vec<(EthereumStorageSchema, H256)>>, String> {
		match self
			.db
			.get(columns::META, &PALLET_ETHEREUM_SCHEMA_CACHE.encode())
		{
			Some(raw) => Ok(Some(
				Decode::decode(&mut &raw[..]).map_err(|e| e.to_string())?,
			)),
			None => Ok(None),
		}
	}

	pub fn write_ethereum_schema(
		&self,
		new_cache: Vec<(EthereumStorageSchema, H256)>,
	) -> Result<(), String> {
		let mut transaction = sp_database::Transaction::new();

		transaction.set(
			columns::META,
			&PALLET_ETHEREUM_SCHEMA_CACHE.encode(),
			&new_cache.encode(),
		);

		self.db.commit(transaction).map_err(|e| e.to_string())?;

		Ok(())
	}
}

#[derive(Debug)]
pub struct MappingCommitment<Block: BlockT> {
	pub block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub ethereum_transaction_hashes: Vec<H256>,
}

pub struct MappingDb<Block> {
	db: Arc<dyn Database<DbHash>>,
	write_lock: Arc<Mutex<()>>,
	_marker: PhantomData<Block>,
}

impl<Block: BlockT> MappingDb<Block> {
	pub fn is_synced(&self, block_hash: &Block::Hash) -> Result<bool, String> {
		match self.db.get(columns::SYNCED_MAPPING, &block_hash.encode()) {
			Some(raw) => Ok(bool::decode(&mut &raw[..]).map_err(|e| format!("{e:?}"))?),
			None => Ok(false),
		}
	}

	pub fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String> {
		match self
			.db
			.get(columns::BLOCK_MAPPING, &ethereum_block_hash.encode())
		{
			Some(raw) => Ok(Some(
				Vec::<Block::Hash>::decode(&mut &raw[..]).map_err(|e| format!("{e:?}"))?,
			)),
			None => Ok(None),
		}
	}

	pub fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String> {
		match self.db.get(
			columns::TRANSACTION_MAPPING,
			&ethereum_transaction_hash.encode(),
		) {
			Some(raw) => Ok(Vec::<TransactionMetadata<Block>>::decode(&mut &raw[..])
				.map_err(|e| e.to_string())?),
			None => Ok(Vec::new()),
		}
	}

	pub fn write_none(&self, block_hash: Block::Hash) -> Result<(), String> {
		let _lock = self.write_lock.lock();

		let mut transaction = sp_database::Transaction::new();

		transaction.set(
			columns::SYNCED_MAPPING,
			&block_hash.encode(),
			&true.encode(),
		);

		self.db.commit(transaction).map_err(|e| e.to_string())?;

		Ok(())
	}

	pub fn write_hashes(
		&self,
		commitment: MappingCommitment<Block>,
		block_number: u64,
	) -> Result<(), String> {
		let _lock = self.write_lock.lock();

		let mut transaction = sp_database::Transaction::new();

		let substrate_hashes = match self.block_hash(&commitment.ethereum_block_hash) {
			Ok(Some(mut data)) => {
				if !data.contains(&commitment.block_hash) {
					data.push(commitment.block_hash);
					log::warn!(
						target: "fc-db",
						"Possible equivocation at ethereum block hash {} {:?}",
						&commitment.ethereum_block_hash,
						&data
					);
				}
				data
			}
			_ => vec![commitment.block_hash],
		};

		transaction.set(
			columns::BLOCK_MAPPING,
			&commitment.ethereum_block_hash.encode(),
			&substrate_hashes.encode(),
		);

		// Write block number -> ethereum block hash mapping
		transaction.set(
			columns::BLOCK_NUMBER_MAPPING,
			&block_number.encode(),
			&commitment.ethereum_block_hash.encode(),
		);

		for (i, ethereum_transaction_hash) in commitment
			.ethereum_transaction_hashes
			.into_iter()
			.enumerate()
		{
			let mut metadata = self.transaction_metadata(&ethereum_transaction_hash)?;
			metadata.push(TransactionMetadata::<Block> {
				substrate_block_hash: commitment.block_hash,
				ethereum_block_hash: commitment.ethereum_block_hash,
				ethereum_index: i as u32,
			});
			transaction.set(
				columns::TRANSACTION_MAPPING,
				&ethereum_transaction_hash.encode(),
				&metadata.encode(),
			);
		}

		transaction.set(
			columns::SYNCED_MAPPING,
			&commitment.block_hash.encode(),
			&true.encode(),
		);

		self.db.commit(transaction).map_err(|e| e.to_string())?;

		Ok(())
	}

	pub fn block_hash_by_number(&self, block_number: u64) -> Result<Option<H256>, String> {
		match self
			.db
			.get(columns::BLOCK_NUMBER_MAPPING, &block_number.encode())
		{
			Some(raw) => Ok(Some(
				H256::decode(&mut &raw[..]).map_err(|e| format!("{e:?}"))?,
			)),
			None => Ok(None),
		}
	}

	/// Returns the latest canonical indexed block number, or None if not set.
	pub fn latest_canonical_indexed_block_number(&self) -> Result<Option<u64>, String> {
		match self
			.db
			.get(columns::META, static_keys::LATEST_CANONICAL_INDEXED_BLOCK)
		{
			Some(raw) => Ok(Some(
				u64::decode(&mut &raw[..]).map_err(|e| format!("{e:?}"))?,
			)),
			None => Ok(None),
		}
	}

	/// Sets the latest canonical indexed block number.
	pub fn set_latest_canonical_indexed_block(&self, block_number: u64) -> Result<(), String> {
		let mut transaction = sp_database::Transaction::new();
		transaction.set(
			columns::META,
			static_keys::LATEST_CANONICAL_INDEXED_BLOCK,
			&block_number.encode(),
		);
		self.db.commit(transaction).map_err(|e| e.to_string())
	}
}
