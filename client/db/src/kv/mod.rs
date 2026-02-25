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
/// Maximum number of blocks inspected in a single recovery pass when the
/// latest indexed canonical pointer is stale or missing.
const INDEXED_RECOVERY_SCAN_LIMIT: u64 = 8192;

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
	pub const CANONICAL_NUMBER_REPAIR_CURSOR: &[u8] = b"CANONICAL_NUMBER_REPAIR_CURSOR";
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

	async fn set_block_hash_by_number(
		&self,
		block_number: u64,
		ethereum_block_hash: H256,
	) -> Result<(), String> {
		self.mapping()
			.set_block_hash_by_number(block_number, ethereum_block_hash)
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

		// Fast path: if best is already indexed and canonical, use it directly.
		if let Some(canonical_hash) = self.indexed_canonical_hash_at(best_number)? {
			self.mapping
				.set_latest_canonical_indexed_block(best_number)?;
			return Ok(canonical_hash);
		}

		// Use persisted latest indexed block when mapping-sync is behind. This avoids
		// falling back to genesis when the chain head has advanced beyond the 8k
		// block scan limit—e.g. on heavily used chains where indexing lags.
		if let Ok(Some(persisted_number)) = self.mapping.latest_canonical_indexed_block_number() {
			if persisted_number <= best_number {
				if let Some(canonical_hash) = self.indexed_canonical_hash_at(persisted_number)? {
					return Ok(canonical_hash);
				}
			}
		}

		// Best block is not indexed yet or mapping is stale (reorg). Walk back to
		// the latest indexed canonical block and persist the recovered pointer.
		if let Some((recovered_number, recovered_hash)) =
			self.find_latest_indexed_canonical_block(best_number.saturating_sub(1))?
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
	/// backwards from `start_block`, bounded to `INDEXED_RECOVERY_SCAN_LIMIT`
	/// probes to keep lookups fast on long chains.
	fn find_latest_indexed_canonical_block(
		&self,
		start_block: u64,
	) -> Result<Option<(u64, Block::Hash)>, String> {
		let scan_limit = INDEXED_RECOVERY_SCAN_LIMIT.saturating_sub(1);
		let min_block = start_block.saturating_sub(scan_limit);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumberMappingWrite {
	Write,
	Skip,
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
		number_mapping_write: NumberMappingWrite,
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

		if number_mapping_write == NumberMappingWrite::Write {
			transaction.set(
				columns::BLOCK_NUMBER_MAPPING,
				&block_number.encode(),
				&commitment.ethereum_block_hash.encode(),
			);
		}

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

	pub fn set_block_hash_by_number(
		&self,
		block_number: u64,
		ethereum_block_hash: H256,
	) -> Result<(), String> {
		let _lock = self.write_lock.lock();

		let mut transaction = sp_database::Transaction::new();
		transaction.set(
			columns::BLOCK_NUMBER_MAPPING,
			&block_number.encode(),
			&ethereum_block_hash.encode(),
		);
		self.db.commit(transaction).map_err(|e| e.to_string())
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

	/// Returns the canonical number-repair cursor, or None if not set.
	pub fn canonical_number_repair_cursor(&self) -> Result<Option<u64>, String> {
		match self
			.db
			.get(columns::META, static_keys::CANONICAL_NUMBER_REPAIR_CURSOR)
		{
			Some(raw) => Ok(Some(
				u64::decode(&mut &raw[..]).map_err(|e| format!("{e:?}"))?,
			)),
			None => Ok(None),
		}
	}

	/// Sets the canonical number-repair cursor.
	pub fn set_canonical_number_repair_cursor(&self, block_number: u64) -> Result<(), String> {
		let mut transaction = sp_database::Transaction::new();
		transaction.set(
			columns::META,
			static_keys::CANONICAL_NUMBER_REPAIR_CURSOR,
			&block_number.encode(),
		);
		self.db.commit(transaction).map_err(|e| e.to_string())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use fc_api::Backend as _;
	use sc_block_builder::BlockBuilderBuilder;
	use sp_consensus::BlockOrigin;
	use sp_core::H256;
	use sp_runtime::{generic::Header, traits::BlakeTwo256, Digest};
	use substrate_test_runtime_client::{
		ClientBlockImportExt, DefaultTestClientBuilderExt, TestClientBuilder,
	};
	use tempfile::tempdir;

	type OpaqueBlock = sp_runtime::generic::Block<
		Header<u64, BlakeTwo256>,
		substrate_test_runtime_client::runtime::Extrinsic,
	>;

	/// Regression test: `eth_blockNumber` must not return 0x00 (`latest_block_hash` must not
	/// fall back to genesis) when mapping-sync has not yet reconciled a re-org and the
	/// `LATEST_CANONICAL_INDEXED_BLOCK` pointer sits at a height whose `BLOCK_NUMBER_MAPPING`
	/// entry still references the retracted fork's Ethereum hash.
	///
	/// Before the fix, `find_latest_indexed_canonical_block` used the cached pointer as a
	/// hard lower bound, collapsing the scan window to just the handful of stale blocks and
	/// causing the fallback path to return genesis (→ 0x00).
	#[tokio::test]
	async fn latest_block_hash_scans_past_stale_reorg_window() {
		let tmp = tempdir().expect("create a temporary directory");
		let (client, _backend) = TestClientBuilder::new().build_with_native_executor::<
			substrate_test_runtime_client::runtime::RuntimeApi,
			_,
		>(None);
		let client = Arc::new(client);

		let frontier_backend = Arc::new(
			Backend::<OpaqueBlock, _>::new(
				client.clone(),
				&DatabaseSettings {
					#[cfg(feature = "rocksdb")]
					source: sc_client_db::DatabaseSource::RocksDb {
						path: tmp.path().to_path_buf(),
						cache_size: 0,
					},
					#[cfg(not(feature = "rocksdb"))]
					source: sc_client_db::DatabaseSource::ParityDb {
						path: tmp.path().to_path_buf(),
					},
				},
			)
			.expect("frontier backend"),
		);

		// Import 5 substrate blocks so `client.hash(n)` resolves real canonical hashes.
		let mut substrate_hashes = vec![client.chain_info().genesis_hash]; // [0] = genesis
		for _ in 1u64..=5 {
			let chain_info = client.chain_info();
			let block = BlockBuilderBuilder::new(&*client)
				.on_parent_block(chain_info.best_hash)
				.with_parent_block_number(chain_info.best_number)
				.with_inherent_digests(Digest::default())
				.build()
				.unwrap()
				.build()
				.unwrap()
				.block;
			let hash = block.header.hash();
			client.import(BlockOrigin::Own, block).await.unwrap();
			substrate_hashes.push(hash);
		}
		// substrate_hashes[n] is the canonical substrate hash for block height n.

		// Write correct frontier mappings for blocks 1..3 (stable, pre-reorg).
		// Each entry links: block_number → eth_hash → [substrate_hash].
		for n in 1u64..=3 {
			let eth_hash = H256::repeat_byte(n as u8);
			let commitment = MappingCommitment::<OpaqueBlock> {
				block_hash: substrate_hashes[n as usize],
				ethereum_block_hash: eth_hash,
				ethereum_transaction_hashes: vec![],
			};
			frontier_backend
				.mapping()
				.write_hashes(commitment, n, NumberMappingWrite::Write)
				.expect("write stable mapping");
		}

		// Simulate a re-org at heights 4 and 5: the BLOCK_NUMBER_MAPPING entries point
		// to Ethereum hashes whose BLOCK_MAPPING entry (eth → substrate) does NOT include
		// the current canonical substrate hash, so indexed_canonical_hash_at() returns
		// None for both heights (stale / retracted-fork state).
		for n in 4u64..=5 {
			// Use a unique hash that was never written into BLOCK_MAPPING.
			let stale_eth_hash = H256::repeat_byte(0xA0 + n as u8);
			frontier_backend
				.mapping()
				.set_block_hash_by_number(n, stale_eth_hash)
				.expect("write stale number mapping");
		}

		// Set the cached LATEST_CANONICAL_INDEXED_BLOCK pointer to 5, as mapping-sync
		// would have after successfully processing blocks up to height 5 before the reorg.
		frontier_backend
			.mapping()
			.set_latest_canonical_indexed_block(5)
			.expect("set latest pointer");

		// The best block is now height 5 (after the reorg, substrate tip = 5).
		assert_eq!(
			client.chain_info().best_number,
			5,
			"test setup: substrate best should be at height 5"
		);

		// latest_block_hash() must walk past the stale blocks at heights 4 and 5 and
		// return the last correctly-indexed canonical block (height 3), not genesis.
		let result = frontier_backend
			.latest_block_hash()
			.await
			.expect("latest_block_hash");

		assert_ne!(
			result,
			client.chain_info().genesis_hash,
			"latest_block_hash must NOT fall back to genesis during a pending re-org reconciliation"
		);
		assert_eq!(
			result, substrate_hashes[3],
			"latest_block_hash should return the highest correctly-indexed canonical block (height 3)"
		);
	}
}
