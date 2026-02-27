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
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
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
#[cfg(not(test))]
const INDEXED_RECOVERY_SCAN_LIMIT: u64 = 8192;
/// Smaller test-only limit so deep-lag branch behavior can be exercised
/// without creating thousands of blocks in unit tests.
#[cfg(test)]
const INDEXED_RECOVERY_SCAN_LIMIT: u64 = 8;
/// Scan limit for the deep-recovery pass when pointer and 32k scan both miss but
/// best > 0. Extends coverage to ~64k blocks from best before falling back to genesis.
const INDEXED_DEEP_RECOVERY_SCAN_LIMIT: u64 = INDEXED_RECOVERY_SCAN_LIMIT * 8;
/// Minimum interval (seconds) between exhaustive-fallback warnings to avoid
/// flooding logs during startup or sustained indexing lag.
const EXHAUSTIVE_FALLBACK_WARN_INTERVAL_SECS: u64 = 60;
/// Epoch-seconds of the last exhaustive-fallback warning. Zero means "never warned".
static LAST_EXHAUSTIVE_WARN_SECS: AtomicU64 = AtomicU64::new(0);

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

		// Walk backwards from best in three layers, each covering a deeper
		// non-overlapping range. The persisted pointer is checked last so it
		// never short-circuits past higher indexed blocks in the scan ranges.
		let bounded_start = best_number.saturating_sub(1);

		// Layer 1 — bounded scan: [best-1 .. best-8k]
		if let Some((found_number, found_hash)) =
			self.find_latest_indexed_canonical_block(bounded_start, INDEXED_RECOVERY_SCAN_LIMIT)?
		{
			self.mapping
				.set_latest_canonical_indexed_block(found_number)?;
			return Ok(found_hash);
		}

		// Layer 2 — exhaustive scan: [best-8k-1 .. best-32k]
		// Extends the search when indexing is far behind. Skip only at genesis.
		if best_number > 0 {
			let now_secs = std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs();
			let prev = LAST_EXHAUSTIVE_WARN_SECS.load(Ordering::Relaxed);
			if now_secs.saturating_sub(prev) >= EXHAUSTIVE_FALLBACK_WARN_INTERVAL_SECS
				&& LAST_EXHAUSTIVE_WARN_SECS
					.compare_exchange(prev, now_secs, Ordering::Relaxed, Ordering::Relaxed)
					.is_ok()
			{
				log::warn!(
					target: "frontier-db",
					"latest_block_hash: exhaustive fallback triggered (best_number={best_number}). If this persists, check indexing progress.",
				);
			}

			// 8k bounded + 24k exhaustive = 32k total non-overlapping coverage.
			let exhaustive_start = bounded_start.saturating_sub(INDEXED_RECOVERY_SCAN_LIMIT);
			let exhaustive_limit = INDEXED_RECOVERY_SCAN_LIMIT * 3;
			if let Some((found_number, found_hash)) =
				self.find_latest_indexed_canonical_block(exhaustive_start, exhaustive_limit)?
			{
				self.mapping
					.set_latest_canonical_indexed_block(found_number)?;
				return Ok(found_hash);
			}
		}

		// Layer 3 — persisted pointer: O(1) jump for extreme lag (>32k blocks).
		// Checked last so layers 1–2 always find the highest indexed block
		// within 32k of best. The pointer only helps when indexing is so far
		// behind that both scans miss entirely.
		//
		// When the pointer target is stale (e.g. reorg), walk backward from it to
		// find the latest valid indexed canonical block instead of falling to genesis.
		if let Some(persisted_number) = self.mapping.latest_canonical_indexed_block_number()? {
			if persisted_number <= best_number {
				if let Some(canonical_hash) = self.indexed_canonical_hash_at(persisted_number)? {
					return Ok(canonical_hash);
				}
				// Pointer target is stale; backtrack from pointer-1 to find a valid block.
				if persisted_number > 0 {
					let backtrack_start = persisted_number.saturating_sub(1);
					if let Some((found_number, found_hash)) = self
						.find_latest_indexed_canonical_block(
							backtrack_start,
							INDEXED_RECOVERY_SCAN_LIMIT,
						)? {
						self.mapping
							.set_latest_canonical_indexed_block(found_number)?;
						return Ok(found_hash);
					}
				}
			}
		}

		// Layer 4 — deep recovery: when best > 0, scan further back before falling to genesis.
		// Covers [best-96k .. best-32k-1] so we avoid returning genesis when indexed data
		// exists just outside the 32k window (e.g. after pointer loss or corruption).
		if best_number > 0 {
			let deep_start = bounded_start
				.saturating_sub(INDEXED_RECOVERY_SCAN_LIMIT)
				.saturating_sub(INDEXED_RECOVERY_SCAN_LIMIT * 3);
			if let Some((found_number, found_hash)) = self
				.find_latest_indexed_canonical_block(deep_start, INDEXED_DEEP_RECOVERY_SCAN_LIMIT)?
			{
				self.mapping
					.set_latest_canonical_indexed_block(found_number)?;
				return Ok(found_hash);
			}
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
	/// backwards from `start_block`, bounded to `scan_limit` probes.
	fn find_latest_indexed_canonical_block(
		&self,
		start_block: u64,
		scan_limit: u64,
	) -> Result<Option<(u64, Block::Hash)>, String> {
		if scan_limit == 0 {
			return Ok(None);
		}
		let min_block = start_block.saturating_sub(scan_limit - 1);
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

	struct TestEnv {
		client: Arc<substrate_test_runtime_client::TestClient>,
		backend: Arc<Backend<OpaqueBlock, substrate_test_runtime_client::TestClient>>,
		substrate_hashes: Vec<<OpaqueBlock as sp_runtime::traits::Block>::Hash>,
		_tmp: tempfile::TempDir,
	}

	impl TestEnv {
		async fn new(num_blocks: u64) -> Self {
			let tmp = tempdir().expect("create a temporary directory");
			let (client, _substrate_backend) = TestClientBuilder::new()
				.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
				None,
			);
			let client = Arc::new(client);

			let backend = Arc::new(
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

			let mut substrate_hashes = vec![client.chain_info().genesis_hash];
			for _ in 1u64..=num_blocks {
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

			Self {
				client,
				backend,
				substrate_hashes,
				_tmp: tmp,
			}
		}

		fn index_block(&self, n: u64) {
			let eth_hash = H256::repeat_byte(n as u8);
			let commitment = MappingCommitment::<OpaqueBlock> {
				block_hash: self.substrate_hashes[n as usize],
				ethereum_block_hash: eth_hash,
				ethereum_transaction_hashes: vec![],
			};
			self.backend
				.mapping()
				.write_hashes(commitment, n, NumberMappingWrite::Write)
				.expect("write mapping");
		}

		fn write_stale_mapping(&self, n: u64) {
			let stale_eth_hash = H256::repeat_byte(0xA0 + n as u8);
			self.backend
				.mapping()
				.set_block_hash_by_number(n, stale_eth_hash)
				.expect("write stale number mapping");
		}

		fn set_pointer(&self, n: u64) {
			self.backend
				.mapping()
				.set_latest_canonical_indexed_block(n)
				.expect("set pointer");
		}

		fn genesis_hash(&self) -> <OpaqueBlock as sp_runtime::traits::Block>::Hash {
			self.client.chain_info().genesis_hash
		}

		async fn latest(&self) -> <OpaqueBlock as sp_runtime::traits::Block>::Hash {
			self.backend
				.latest_block_hash()
				.await
				.expect("latest_block_hash")
		}
	}

	#[tokio::test]
	async fn fast_path_returns_best_when_fully_indexed() {
		let env = TestEnv::new(5).await;
		for n in 1u64..=5 {
			env.index_block(n);
		}
		env.set_pointer(5);

		let result = env.latest().await;
		assert_eq!(result, env.substrate_hashes[5]);
	}

	#[tokio::test]
	async fn bounded_scan_finds_latest_indexed_under_normal_lag() {
		let env = TestEnv::new(10).await;
		for n in 1u64..=7 {
			env.index_block(n);
		}
		env.set_pointer(7);

		let result = env.latest().await;
		assert_eq!(
			result, env.substrate_hashes[7],
			"should find block 7 via bounded scan even though best is 10"
		);
	}

	#[tokio::test]
	async fn bounded_scan_prefers_newer_over_stale_pointer() {
		let env = TestEnv::new(10).await;
		// Simulate: pointer was set to 3 a while ago, but mapping-sync has since
		// indexed up to 8. The bounded scan must find 8, not return the stale 3.
		for n in 1u64..=8 {
			env.index_block(n);
		}
		env.set_pointer(3);

		let result = env.latest().await;
		assert_eq!(
			result, env.substrate_hashes[8],
			"bounded scan must find block 8, not return stale pointer at 3"
		);
	}

	#[tokio::test]
	async fn reorg_with_stale_pointer_walks_past_stale_blocks() {
		let env = TestEnv::new(5).await;
		for n in 1u64..=3 {
			env.index_block(n);
		}
		for n in 4u64..=5 {
			env.write_stale_mapping(n);
		}
		env.set_pointer(5);

		let result = env.latest().await;
		assert_ne!(result, env.genesis_hash(), "must not fall back to genesis");
		assert_eq!(
			result, env.substrate_hashes[3],
			"should return block 3 (highest valid indexed block)"
		);
	}

	#[tokio::test]
	async fn no_pointer_still_finds_indexed_blocks() {
		let env = TestEnv::new(5).await;
		for n in 1u64..=3 {
			env.index_block(n);
		}
		// No pointer set — simulates DB corruption or first run after pointer loss.

		let result = env.latest().await;
		assert_eq!(
			result, env.substrate_hashes[3],
			"should find block 3 via bounded scan even without a pointer"
		);
	}

	#[tokio::test]
	async fn initial_sync_nothing_indexed_returns_genesis() {
		let env = TestEnv::new(5).await;
		// No blocks indexed, no pointer.

		let result = env.latest().await;
		assert_eq!(
			result,
			env.genesis_hash(),
			"should return genesis when nothing is indexed"
		);
	}

	#[tokio::test]
	async fn genesis_only_returns_genesis() {
		let env = TestEnv::new(0).await;

		let result = env.latest().await;
		assert_eq!(
			result,
			env.genesis_hash(),
			"should return genesis when chain is at block 0"
		);
	}

	#[tokio::test]
	async fn exhaustive_scan_finds_indexed_block_beyond_bounded_range() {
		// With the test scan limit (8), best=20 yields:
		// - bounded scan over [12..19]
		// - exhaustive scan over [0..11]
		let env = TestEnv::new(20).await;
		env.index_block(5);

		let result = env.latest().await;
		assert_eq!(
			result, env.substrate_hashes[5],
			"should recover block 5 via exhaustive scan when bounded scan misses"
		);
	}

	#[tokio::test]
	async fn persisted_pointer_used_when_both_scan_layers_miss() {
		// With the test scan limit (8), best=40 covers:
		// - bounded [32..39]
		// - exhaustive [8..31]
		// So block 3 is only reachable via the persisted pointer fallback.
		let env = TestEnv::new(40).await;
		env.index_block(3);
		env.set_pointer(3);

		let result = env.latest().await;
		assert_eq!(
			result, env.substrate_hashes[3],
			"should use persisted pointer when bounded+exhaustive scans both miss"
		);
	}

	#[tokio::test]
	async fn deep_recovery_finds_indexed_block_when_pointer_missing() {
		// With scan limit 8: bounded [32..39], exhaustive [8..31]. Block 3 is outside both.
		// Layer 4 deep recovery [0..7] finds it when no pointer exists.
		let env = TestEnv::new(40).await;
		env.index_block(3);

		let result = env.latest().await;
		assert_ne!(result, env.genesis_hash(), "must not fall back to genesis");
		assert_eq!(
			result, env.substrate_hashes[3],
			"deep recovery should find block 3 when pointer is missing"
		);
	}

	#[tokio::test]
	async fn pointer_above_best_ignored_deep_recovery_finds_block() {
		// Pointer corruption: pointer > best is ignored. Deep recovery should still
		// find indexed block 3 in [0..7] when bounded+exhaustive miss.
		let env = TestEnv::new(40).await;
		env.index_block(3);
		env.set_pointer(100);

		let result = env.latest().await;
		assert_ne!(result, env.genesis_hash(), "must not fall back to genesis");
		assert_eq!(
			result, env.substrate_hashes[3],
			"deep recovery should find block 3 when pointer is invalid"
		);
	}

	#[tokio::test]
	async fn genesis_fallback_when_indexed_block_outside_all_windows() {
		// With limit 8: deep recovery covers [best-96..best-33]. For best=100, [4..67].
		// Block 2 is outside; no pointer. Documents that genesis is still returned when
		// indexed data exists but is beyond even the deep-recovery window.
		let env = TestEnv::new(100).await;
		env.index_block(2);

		let result = env.latest().await;
		assert_eq!(
			result,
			env.genesis_hash(),
			"indexed block outside all scan windows with no pointer yields genesis"
		);
	}

	#[tokio::test]
	async fn stale_pointer_target_backtracks_to_find_valid_block() {
		// Pointer points to a stale/unusable number mapping (no canonical indexed
		// block at that height). There is an older valid indexed block (2). The
		// resolver must backtrack from pointer-1 and return block 2, not genesis.
		let env = TestEnv::new(40).await;
		env.index_block(2);
		env.write_stale_mapping(3);
		env.set_pointer(3);

		let result = env.latest().await;
		assert_ne!(result, env.genesis_hash(), "must not fall back to genesis");
		assert_eq!(
			result, env.substrate_hashes[2],
			"should find block 2 via backtrack from stale pointer target"
		);
	}

	#[tokio::test]
	async fn pointer_updates_after_stale_pointer_backtrack_recovery() {
		// After backtrack from a stale pointer, the persisted pointer should be
		// updated to the block we found.
		let env = TestEnv::new(40).await;
		env.index_block(2);
		env.write_stale_mapping(3);
		env.set_pointer(3);

		let _ = env.latest().await;

		let updated = env
			.backend
			.mapping()
			.latest_canonical_indexed_block_number()
			.expect("read pointer");
		assert_eq!(
			updated,
			Some(2),
			"pointer should be updated to block 2 found by backtrack"
		);
	}

	#[tokio::test]
	async fn pointer_updates_after_bounded_scan_recovery() {
		let env = TestEnv::new(10).await;
		for n in 1u64..=6 {
			env.index_block(n);
		}
		env.set_pointer(3);

		let _ = env.latest().await;

		// After the call, the pointer should have been updated to 6.
		let updated = env
			.backend
			.mapping()
			.latest_canonical_indexed_block_number()
			.expect("read pointer");
		assert_eq!(
			updated,
			Some(6),
			"pointer should be updated to the block found by bounded scan"
		);
	}
}
