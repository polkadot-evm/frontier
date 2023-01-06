// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
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

use scale_codec::{Decode, Encode};
use fp_consensus::FindLogError;
use fp_rpc::EthereumRuntimeRPCApi;
use fp_storage::{EthereumStorageSchema, OverrideHandle, PALLET_ETHEREUM_SCHEMA};
use sc_client_api::backend::{Backend as BackendT, StateBackend, StorageProvider};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::{H160, H256};
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT, Header as HeaderT, UniqueSaturatedInto, Zero},
};
use sqlx::{
	query::Query,
	sqlite::{
		SqliteArguments, SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteQueryResult,
	},
	ConnectOptions, Error, Execute, QueryBuilder, Row, Sqlite,
};

use std::{str::FromStr, sync::Arc};

use crate::FilteredLog;

#[derive(Debug, Eq, PartialEq)]
pub struct Log {
	pub address: Vec<u8>,
	pub topic_1: Vec<u8>,
	pub topic_2: Vec<u8>,
	pub topic_3: Vec<u8>,
	pub topic_4: Vec<u8>,
	pub log_index: i32,
	pub transaction_index: i32,
	pub substrate_block_hash: Vec<u8>,
}

#[derive(Eq, PartialEq)]
struct BlockMetadata {
	pub substrate_block_hash: H256,
	pub block_number: i32,
	pub post_hashes: fp_consensus::Hashes,
	pub schema: EthereumStorageSchema,
	pub is_canon: i32,
}

pub struct SqliteBackendConfig<'a> {
	pub path: &'a str,
	pub create_if_missing: bool,
}

pub enum BackendConfig<'a> {
	Sqlite(SqliteBackendConfig<'a>),
}

#[derive(Clone)]
pub struct Backend<Block: BlockT> {
	pool: SqlitePool,
	overrides: Arc<OverrideHandle<Block>>,
	num_ops_timeout: i32,
}
impl<Block: BlockT> Backend<Block>
where
	Block: BlockT<Hash = H256> + Send + Sync,
{
	pub async fn new(
		config: BackendConfig<'_>,
		pool_size: u32,
		num_ops_timeout: u32,
		overrides: Arc<OverrideHandle<Block>>,
	) -> Result<Self, Error> {
		let any_pool = SqlitePoolOptions::new()
			.max_connections(pool_size)
			.connect_lazy_with(
				Self::connect_options(&config)?
					.disable_statement_logging()
					.clone(),
			);
		let _ = Self::create_if_not_exists(&any_pool).await?;
		Ok(Self {
			pool: any_pool,
			overrides,
			num_ops_timeout: num_ops_timeout.try_into().unwrap_or(i32::MAX),
		})
	}

	fn connect_options(config: &BackendConfig) -> Result<SqliteConnectOptions, Error> {
		match config {
			BackendConfig::Sqlite(config) => {
				let config = sqlx::sqlite::SqliteConnectOptions::from_str(config.path)?
					.create_if_missing(config.create_if_missing)
					// https://www.sqlite.org/pragma.html#pragma_busy_timeout
					.busy_timeout(std::time::Duration::from_secs(8))
					// https://www.sqlite.org/pragma.html#pragma_analysis_limit
					.pragma("analysis_limit", "1000")
					// https://www.sqlite.org/pragma.html#pragma_threads
					.pragma("threads", "4")
					// https://www.sqlite.org/pragma.html#pragma_threads
					.pragma("temp_store", "memory")
					// https://www.sqlite.org/wal.html
					.journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
					// https://www.sqlite.org/pragma.html#pragma_synchronous
					.synchronous(sqlx::sqlite::SqliteSynchronous::Normal);
				Ok(config)
			}
		}
	}

	pub fn pool(&self) -> &SqlitePool {
		&self.pool
	}

	pub async fn canonicalize(&self, retracted: &[H256], enacted: &[H256]) -> Result<(), Error> {
		let mut tx = self.pool().begin().await?;

		// Retracted
		let mut builder: QueryBuilder<Sqlite> =
			QueryBuilder::new("UPDATE blocks SET is_canon = 0 WHERE substrate_block_hash IN (");
		let mut retracted_hashes = builder.separated(", ");
		for hash in retracted.iter() {
			let hash = hash.as_bytes().to_owned();
			retracted_hashes.push_bind(hash);
		}
		retracted_hashes.push_unseparated(")");
		let query = builder.build();
		query.execute(&mut tx).await?;

		// Enacted
		let mut builder: QueryBuilder<Sqlite> =
			QueryBuilder::new("UPDATE blocks SET is_canon = 1 WHERE substrate_block_hash IN (");
		let mut enacted_hashes = builder.separated(", ");
		for hash in enacted.iter() {
			let hash = hash.as_bytes().to_owned();
			enacted_hashes.push_bind(hash);
		}
		enacted_hashes.push_unseparated(")");
		let query = builder.build();
		query.execute(&mut tx).await?;

		tx.commit().await
	}

	pub async fn insert_genesis_block_metadata<Client, BE>(
		&self,
		client: Arc<Client>,
	) -> Result<Option<H256>, Error>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		Client: ProvideRuntimeApi<Block>,
		Client::Api: EthereumRuntimeRPCApi<Block>,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let id = BlockId::Number(Zero::zero());
		let substrate_genesis_hash = client
			.expect_block_hash_from_id(&id)
			.map_err(|_| Error::Protocol("Cannot resolve genesis hash".to_string()))?;
		let maybe_substrate_hash: Option<H256> = if let Ok(Some(_)) = client.header(substrate_genesis_hash)
		{
			let has_api = client
				.runtime_api()
				.has_api::<dyn EthereumRuntimeRPCApi<Block>>(&id)
				.expect("runtime api reachable");

			if has_api {
				// The chain has frontier support from genesis.
				// Read from the runtime and store the block metadata.
				let ethereum_block = client
					.runtime_api()
					.current_block(&id)
					.expect("runtime api reachable")
					.expect("ethereum genesis block");

				let schema = Self::onchain_storage_schema(client.as_ref(), substrate_genesis_hash).encode();
				let ethereum_block_hash = ethereum_block.header.hash().as_bytes().to_owned();
				let substrate_block_hash = substrate_genesis_hash.as_bytes().to_owned();
				let block_number = 0i32;
				let is_canon = 1i32;

				let _ = sqlx::query!(
					"INSERT OR IGNORE INTO blocks(
						ethereum_block_hash,
						substrate_block_hash,
						block_number,
						ethereum_storage_schema,
						is_canon)
					VALUES (?, ?, ?, ?, ?)",
					ethereum_block_hash,
					substrate_block_hash,
					block_number,
					schema,
					is_canon,
				)
				.execute(self.pool())
				.await?;
			}
			Some(substrate_genesis_hash)
		} else {
			None
		};
		Ok(maybe_substrate_hash)
	}

	fn insert_block_metadata_inner<Client, BE>(
		client: Arc<Client>,
		hashes: &Vec<H256>,
	) -> Result<Vec<BlockMetadata>, Error>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		log::debug!(
			target: "frontier-sql",
			"🛠️  [Metadata] Retrieving digest data for {:?} block hashes",
			hashes.len()
		);
		let mut out = Vec::new();
		for &hash in hashes.iter() {
			if let Ok(Some(header)) = client.header(hash) {
				match fp_consensus::find_log(header.digest()) {
					Ok(log) => {
						let block_number = *header.number();
						let is_canon: i32 = if let Ok(Some(inner_hash)) = client.hash(block_number) {
							if inner_hash == hash {
								1
							} else {
								0
							}
						} else {
							return Err(Error::Protocol(format!(
								"[Metadata] Failed to retrieve header for number {}",
								block_number
							)));
						};
						let block_number: i32 = UniqueSaturatedInto::<u32>::unique_saturated_into(block_number) as i32;
						let schema = Self::onchain_storage_schema(client.as_ref(), hash);
						out.push(BlockMetadata {
							substrate_block_hash: hash,
							block_number,
							post_hashes: log.into_hashes(),
							schema,
							is_canon,
						});
					}
					Err(FindLogError::NotFound) => {}
					Err(FindLogError::MultipleLogs) => {
						return Err(Error::Protocol(format!(
							"[Metadata] Multiple logs found for hash {}",
							hash
						)))
					}
				}
			}
		}
		log::debug!(
			target: "frontier-sql",
			"🛠️  [Metadata] Retrieved digest data",
		);
		Ok(out)
	}

	pub async fn insert_block_metadata<Client, BE>(
		&self,
		client: Arc<Client>,
		hashes: &Vec<H256>,
	) -> Result<(), Error>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		// Spawn a blocking task to get block metadata from substrate backend.
		let hashes_inner = hashes.clone();
		let block_metadata = tokio::task::spawn_blocking(move || {
			Self::insert_block_metadata_inner(client.clone(), &hashes_inner)
		})
		.await
		.map_err(|_| Error::Protocol("tokio blocking metadata task failed".to_string()))??;

		let mut tx = self.pool().begin().await?;

		log::debug!(
			target: "frontier-sql",
			"🛠️  [Metadata] Starting execution of statements on db transaction"
		);
		for metadata in block_metadata.into_iter() {
			let post_hashes = metadata.post_hashes;
			let ethereum_block_hash = post_hashes.block_hash.as_bytes().to_owned();
			let substrate_block_hash = metadata.substrate_block_hash.as_bytes().to_owned();
			let schema = metadata.schema.encode();
			let block_number = metadata.block_number;
			let is_canon = metadata.is_canon;

			let _ = sqlx::query!(
				"INSERT OR IGNORE INTO blocks(
					ethereum_block_hash,
					substrate_block_hash,
					block_number,
					ethereum_storage_schema,
					is_canon)
				VALUES (?, ?, ?, ?, ?)",
				ethereum_block_hash,
				substrate_block_hash,
				block_number,
				schema,
				is_canon,
			)
			.execute(&mut tx)
			.await?;
			for (i, &transaction_hash) in post_hashes.transaction_hashes.iter().enumerate() {
				let ethereum_transaction_hash = transaction_hash.as_bytes().to_owned();
				let ethereum_transaction_index = i as i32;
				let _ = sqlx::query!(
					"INSERT OR IGNORE INTO transactions(
						ethereum_transaction_hash,
						substrate_block_hash,
						ethereum_block_hash,
						ethereum_transaction_index)
					VALUES (?, ?, ?, ?)",
					ethereum_transaction_hash,
					substrate_block_hash,
					ethereum_block_hash,
					ethereum_transaction_index,
				)
				.execute(&mut tx)
				.await?;
			}
		}

		let mut builder: QueryBuilder<Sqlite> =
			QueryBuilder::new("INSERT INTO sync_status(substrate_block_hash) ");
		builder.push_values(hashes, |mut b, hash| {
			b.push_bind(hash.as_bytes());
		});
		let query = builder.build();
		query.execute(&mut tx).await?;

		log::debug!(
			target: "frontier-sql",
			"🛠️  [Metadata] Ready to commit",
		);
		tx.commit().await
	}

	pub async fn spawn_logs_task<Client, BE>(&self, client: Arc<Client>, batch_size: usize)
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let pool = self.pool().clone();
		let overrides = self.overrides.clone();
		let _ = async {
			// The overarching db transaction for the task.
			// Due to the async nature of this task, the same work is likely to happen
			// more than once. For example when a new batch is scheduled when the previous one
			// didn't finished yet and the new batch happens to select the same substrate
			// block hashes for the update.
			// That is expected, we are exchanging extra work for *acid*ity.
			// There is no case of unique constrain violation or race condition as already
			// existing entries are ignored.
			let mut tx = pool.begin().await?;
			// Update statement returning the substrate block hashes for this batch.
			let q = format!(
				"UPDATE sync_status
				SET status = 1
				WHERE substrate_block_hash IN
					(SELECT substrate_block_hash
					FROM sync_status
					WHERE status = 0
					LIMIT {}) RETURNING substrate_block_hash",
				batch_size
			);
			match sqlx::query(&q).fetch_all(&mut tx).await {
				Ok(result) => {
					let mut to_index: Vec<H256> = vec![];
					for row in result.iter() {
						if let Ok(bytes) = row.try_get::<Vec<u8>, _>(0) {
							to_index.push(H256::from_slice(&bytes[..]));
						} else {
							log::error!(
								target: "frontier-sql",
								"unable to decode row value"
							);
						}
					}
					// Spawn a blocking task to get log data from substrate backend.
					let logs = tokio::task::spawn_blocking(move || {
						Self::spawn_logs_task_inner(client.clone(), overrides, &to_index)
					})
					.await
					.map_err(|_| Error::Protocol("tokio blocking task failed".to_string()))?;

					// TODO VERIFY statements limit per transaction in sqlite if any
					for log in logs.iter() {
						let _ = sqlx::query!(
							"INSERT OR IGNORE INTO logs(
								address,
								topic_1,
								topic_2,
								topic_3,
								topic_4,
								log_index,
								transaction_index,
								substrate_block_hash)
							VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
							log.address,
							log.topic_1,
							log.topic_2,
							log.topic_3,
							log.topic_4,
							log.log_index,
							log.transaction_index,
							log.substrate_block_hash,
						)
						.execute(&mut tx)
						.await?;
					}
					Ok(tx.commit().await?)
				}
				Err(e) => Err(e),
			}
		}
		.await
		.map_err(|e| {
			log::error!(
				target: "frontier-sql",
				"{}",
				e
			)
		});
		// https://www.sqlite.org/pragma.html#pragma_optimize
		let _ = sqlx::query("PRAGMA optimize").execute(&pool).await;
		log::debug!(
			target: "frontier-sql",
			"🛠️  Batch commited"
		);
	}

	fn spawn_logs_task_inner<Client, BE>(
		client: Arc<Client>,
		overrides: Arc<OverrideHandle<Block>>,
		hashes: &[H256],
	) -> Vec<Log>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let mut logs: Vec<Log> = vec![];
		let mut transaction_count: usize = 0;
		let mut log_count: usize = 0;
		for substrate_block_hash in hashes.iter() {
			let id = BlockId::Hash(*substrate_block_hash);
			let schema = Self::onchain_storage_schema(client.as_ref(), *substrate_block_hash);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let receipts = handler.current_receipts(&id).unwrap_or_default();

			transaction_count += receipts.len();
			for (transaction_index, receipt) in receipts.iter().enumerate() {
				let receipt_logs = match receipt {
					ethereum::ReceiptV3::Legacy(d)
					| ethereum::ReceiptV3::EIP2930(d)
					| ethereum::ReceiptV3::EIP1559(d) => &d.logs,
				};
				let transaction_index = transaction_index as i32;
				log_count += receipt_logs.len();
				for (log_index, log) in receipt_logs.iter().enumerate() {
					logs.push(Log {
						address: log.address.as_bytes().to_owned(),
						topic_1: log
							.topics
							.get(0)
							.unwrap_or(&H256::zero())
							.as_bytes()
							.to_owned(),
						topic_2: log
							.topics
							.get(1)
							.unwrap_or(&H256::zero())
							.as_bytes()
							.to_owned(),
						topic_3: log
							.topics
							.get(2)
							.unwrap_or(&H256::zero())
							.as_bytes()
							.to_owned(),
						topic_4: log
							.topics
							.get(3)
							.unwrap_or(&H256::zero())
							.as_bytes()
							.to_owned(),
						log_index: log_index as i32,
						transaction_index,
						substrate_block_hash: substrate_block_hash.as_bytes().to_owned(),
					});
				}
			}
		}
		log::debug!(
			target: "frontier-sql",
			"🛠️  Ready to commit {} logs from {} transactions",
			log_count,
			transaction_count
		);
		logs
	}

	fn onchain_storage_schema<Client, BE>(
		client: &Client,
		at: Block::Hash,
	) -> EthereumStorageSchema
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		match client.storage(
			at,
			&sp_storage::StorageKey(PALLET_ETHEREUM_SCHEMA.to_vec()),
		) {
			Ok(Some(bytes)) => Decode::decode(&mut &bytes.0[..])
				.ok()
				.unwrap_or(EthereumStorageSchema::Undefined),
			_ => EthereumStorageSchema::Undefined,
		}
	}

	async fn create_if_not_exists(pool: &SqlitePool) -> Result<SqliteQueryResult, Error> {
		sqlx::query(
			"BEGIN;
			CREATE TABLE IF NOT EXISTS logs (
				id INTEGER PRIMARY KEY,
				address BLOB NOT NULL,
				topic_1 BLOB NOT NULL,
				topic_2 BLOB NOT NULL,
				topic_3 BLOB NOT NULL,
				topic_4 BLOB NOT NULL,
				log_index INTEGER NOT NULL,
				transaction_index INTEGER NOT NULL,
				substrate_block_hash BLOB NOT NULL,
				UNIQUE (
					log_index,
					transaction_index,
					substrate_block_hash
				)
			);
			CREATE TABLE IF NOT EXISTS sync_status (
				id INTEGER PRIMARY KEY,
				substrate_block_hash BLOB NOT NULL,
				status INTEGER DEFAULT 0 NOT NULL,
				UNIQUE (
					substrate_block_hash
				)
			);
			CREATE TABLE IF NOT EXISTS blocks (
				id INTEGER PRIMARY KEY,
				block_number INTEGER NOT NULL,
				ethereum_block_hash BLOB NOT NULL,
				substrate_block_hash BLOB NOT NULL,
				ethereum_storage_schema BLOB NOT NULL,
				is_canon INTEGER NOT NULL,
				UNIQUE (
					ethereum_block_hash,
					substrate_block_hash
				)
			);
			CREATE TABLE IF NOT EXISTS transactions (
				id INTEGER PRIMARY KEY,
				ethereum_transaction_hash BLOB NOT NULL,
				substrate_block_hash BLOB NOT NULL,
				ethereum_block_hash BLOB NOT NULL,
				ethereum_transaction_index INTEGER NOT NULL,
				UNIQUE (
					ethereum_transaction_hash,
					substrate_block_hash
				)
			);
			COMMIT;",
		)
		.execute(pool)
		.await
	}

	pub async fn create_indexes(&self) -> Result<SqliteQueryResult, Error> {
		sqlx::query(
			"BEGIN;
			CREATE INDEX IF NOT EXISTS logs_main_idx ON logs (
				address,
				topic_1,
				topic_2,
				topic_3,
				topic_4
			);
			CREATE INDEX IF NOT EXISTS logs_substrate_index ON logs (
				substrate_block_hash
			);
			CREATE INDEX IF NOT EXISTS blocks_number_index ON blocks (
				block_number
			);
			CREATE INDEX IF NOT EXISTS blocks_substrate_index ON blocks (
				substrate_block_hash
			);
			CREATE INDEX IF NOT EXISTS eth_block_hash_idx ON blocks (
				ethereum_block_hash
			);
			CREATE INDEX IF NOT EXISTS eth_tx_hash_idx ON transactions (
				ethereum_transaction_hash
			);
			CREATE INDEX IF NOT EXISTS eth_tx_hash_2_idx ON transactions (
				ethereum_block_hash,
				ethereum_transaction_index
			);
			COMMIT;",
		)
		.execute(self.pool())
		.await
	}
}
#[derive(Debug)]
enum FilterValue {
	Address(H160),
	Topic(Option<H256>),
}

#[async_trait::async_trait]
impl<Block: BlockT<Hash = H256>> crate::BackendReader<Block> for Backend<Block> {
	async fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String> {
		let ethereum_block_hash = ethereum_block_hash.as_bytes().to_owned();
		let res = match sqlx::query(
			"SELECT substrate_block_hash FROM blocks WHERE ethereum_block_hash = ?",
		)
		.bind(ethereum_block_hash)
		.fetch_all(&self.pool)
		.await
		{
			Ok(result) => {
				let mut out = vec![];
				for row in result {
					out.push(H256::from_slice(
						&row.try_get::<Vec<u8>, _>(0).unwrap_or_default()[..],
					));
				}
				Some(out)
			}
			_ => None,
		};
		Ok(res)
	}
	async fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<crate::TransactionMetadata<Block>>, String> {
		let mut out = vec![];
		let ethereum_transaction_hash = ethereum_transaction_hash.as_bytes().to_owned();
		if let Ok(result) = sqlx::query(
			"SELECT
				substrate_block_hash, ethereum_block_hash, ethereum_transaction_index
			FROM transactions WHERE ethereum_transaction_hash = ?",
		)
		.bind(ethereum_transaction_hash)
		.fetch_all(&self.pool)
		.await
		{
			for row in result {
				let substrate_block_hash =
					H256::from_slice(&row.try_get::<Vec<u8>, _>(0).unwrap_or_default()[..]);
				let ethereum_block_hash =
					H256::from_slice(&row.try_get::<Vec<u8>, _>(1).unwrap_or_default()[..]);
				let ethereum_transaction_index =
					row.try_get::<i32, _>(2).unwrap_or_default() as u32;
				out.push(crate::TransactionMetadata {
					block_hash: substrate_block_hash,
					ethereum_block_hash,
					ethereum_index: ethereum_transaction_index,
				});
			}
		}
		Ok(out)
	}
	async fn filter_logs(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Vec<H160>,
		topics: Vec<Vec<Option<H256>>>,
	) -> Result<Vec<FilteredLog>, String> {
		let key = format!("{}-{}-{:?}-{:?}", from_block, to_block, addresses, topics);
		log::info!("FILTER {}", key);

		let mut qb = QueryBuilder::new("");
		let query = build_sqlite_query(&mut qb, from_block, to_block, addresses, topics);
		let sql = query.sql();

		let mut out: Vec<FilteredLog> = vec![];
		let mut conn = self
			.pool()
			.acquire()
			.await
			.map_err(|err| format!("failed acquiring sqlite connection: {}", err))?;
		let y = key.clone();
		conn.set_progress_handler(self.num_ops_timeout, move || {
			log::info!(
				target: "frontier-sql",
				"TRIGGER sqlite progress_handler_triggered after {}",
				y,
			);
			false
		})
		.await;
		log::info!(
			target: "frontier-sql",
			"🛠️  Using num_ops_timeout = {} - {}",
			self.num_ops_timeout,
			key,
		);

		log::info!(
			target: "frontier-sql",
			"SQL {:?}",
			sql,
		);

		match query.fetch_all(&mut conn).await {
			Ok(result) => {
				for row in result.iter() {
					// Substrate block hash
					let substrate_block_hash =
						H256::from_slice(&row.try_get::<Vec<u8>, _>(0).unwrap_or_default()[..]);
					// Ethereum block hash
					let ethereum_block_hash =
						H256::from_slice(&row.try_get::<Vec<u8>, _>(1).unwrap_or_default()[..]);
					// Block number
					let block_number = row.try_get::<i32, _>(2).unwrap_or_default() as u32;
					// Ethereum storage schema
					let ethereum_storage_schema: EthereumStorageSchema =
						Decode::decode(&mut &row.try_get::<Vec<u8>, _>(3).unwrap_or_default()[..])
							.map_err(|_| {
								"Cannot decode EthereumStorageSchema for block".to_string()
							})?;
					// Transaction index
					let transaction_index = row.try_get::<i32, _>(4).unwrap_or_default() as u32;
					// Log index
					let log_index = row.try_get::<i32, _>(5).unwrap_or_default() as u32;
					out.push(FilteredLog {
						substrate_block_hash,
						ethereum_block_hash,
						block_number,
						ethereum_storage_schema,
						transaction_index,
						log_index,
					});
				}
			}
			Err(err) => {
				conn.remove_progress_handler().await;
				log::error!(
					target: "frontier-sql",
					"Failed to query sql db with statement {:?} : {:?} - {}",
					// sql,
					"SQL",
					err,
					key,
				);
				return Err("Failed to query sql db with statement".to_string());
			}
		};

		conn.remove_progress_handler().await;
		log::info!(
			target: "frontier-sql",
			"FILTER remove handler - {}",
			key,
		);
		Ok(out)
	}

	fn is_indexed(&self) -> bool {
		true
	}
}

fn build_sqlite_query<'a>(
	qb: &'a mut QueryBuilder<Sqlite>,
	from_block: u64,
	to_block: u64,
	addresses: Vec<H160>,
	topics: Vec<Vec<Option<H256>>>,
) -> Query<'a, Sqlite, SqliteArguments<'a>> {
	qb.push(
		"
SELECT
	l.substrate_block_hash,
	b.ethereum_block_hash,
	b.block_number,
	b.ethereum_storage_schema,
	l.transaction_index,
	l.log_index
FROM logs AS l
INNER JOIN blocks AS b
ON (b.block_number BETWEEN ",
	);
	qb.separated(" AND ")
		.push_bind(from_block as i64)
		.push_bind(to_block as i64)
		.push_unseparated(")");
	qb.push(" AND b.substrate_block_hash = l.substrate_block_hash")
		.push(" AND b.is_canon = 1")
		.push("\nWHERE l.address IN (");
	let mut qb_addr = qb.separated(", ");
	addresses.iter().for_each(|addr| {
		qb_addr.push_bind(addr.as_bytes().to_owned());
	});
	qb_addr.push_unseparated(")");

	for (i, topic_options) in topics.iter().enumerate() {
		let valid_topic_options = topic_options
			.iter()
			.filter(|x| x.is_some())
			.map(|x| x.expect("value must exist as `Some` filtered above; qed"))
			.collect::<Vec<_>>();
		if valid_topic_options.len() == 1 {
			qb.push(format!(" AND l.topic_{} = ", i + 1))
				.push_bind(valid_topic_options[0].as_bytes().to_owned());
		} else if valid_topic_options.len() > 1 {
			qb.push(format!(" AND l.topic_{} IN (", i + 1));
			let mut qb_topic = qb.separated(", ");
			valid_topic_options.iter().for_each(|t| {
				qb_topic.push_bind(t.as_bytes().to_owned());
			});
			qb_topic.push_unseparated(")");
		}
	}

	qb.push(
		"
GROUP BY l.substrate_block_hash, l.transaction_index, l.log_index
ORDER BY l.block_number ASC, l.transaction_index ASC, l.log_index ASC
LIMIT 10001",
	);

	qb.build()
}

#[cfg(test)]
mod test {

	use super::FilteredLog;

	use crate::BackendReader;
	use codec::Encode;
	use fc_rpc::{SchemaV3Override, StorageOverride};
	use fp_storage::{EthereumStorageSchema, OverrideHandle, PALLET_ETHEREUM_SCHEMA};
	use sp_core::{H160, H256};
	use sp_runtime::{
		generic::{Block, Header},
		traits::BlakeTwo256,
	};
	use sqlx::QueryBuilder;
	use std::{collections::BTreeMap, path::Path, str::FromStr, sync::Arc};
	use substrate_test_runtime_client::{
		DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	use tempfile::tempdir;

	type OpaqueBlock =
		Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

	struct TestFilter {
		pub from_block: u64,
		pub to_block: u64,
		pub addresses: Vec<H160>,
		pub topics: Vec<Vec<Option<H256>>>,
		pub expected_result: Vec<FilteredLog>,
	}

	struct TestData {
		pub alice: H160,
		pub bob: H160,
		pub topics_a: H256,
		pub topics_b: H256,
		pub topics_c: H256,
		pub topics_d: H256,
		pub substrate_hash_1: H256,
		pub substrate_hash_2: H256,
		pub substrate_hash_3: H256,
		pub ethereum_hash_1: H256,
		pub ethereum_hash_2: H256,
		pub ethereum_hash_3: H256,
		pub backend: super::Backend<OpaqueBlock>,
	}

	// From `(substrate_block_hash, transaction_index, log_index)` to FilteredLog
	impl From<(H256, H256, u32, u32, u32)> for FilteredLog {
		fn from(values: (H256, H256, u32, u32, u32)) -> Self {
			Self {
				substrate_block_hash: values.0,
				ethereum_block_hash: values.1,
				block_number: values.2,
				ethereum_storage_schema: EthereumStorageSchema::V3,
				transaction_index: values.3,
				log_index: values.4,
			}
		}
	}
	async fn prepare() -> TestData {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V3
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		// Client
		let (client, _) = builder
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
				None,
			);
		let client = Arc::new(client);
		// Overrides
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone()))
				as Box<dyn StorageOverride<_> + Send + Sync>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});
		// Indexer backend
		let indexer_backend = super::Backend::new(
			super::BackendConfig::Sqlite(super::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path().strip_prefix("/").unwrap().to_str().unwrap())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
			}),
			100,
			0,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Prepare test db data
		// Addresses
		let alice = H160::random();
		let bob = H160::random();
		// Topics
		let topics_a = H256::random();
		let topics_b = H256::random();
		let topics_c = H256::random();
		let topics_d = H256::random();
		// Substrate block hashes
		let substrate_hash_1 = H256::random();
		let substrate_hash_2 = H256::random();
		let substrate_hash_3 = H256::random();
		// Ethereum block hashes
		let ethereum_hash_1 = H256::random();
		let ethereum_hash_2 = H256::random();
		let ethereum_hash_3 = H256::random();
		// Ethereum storage schema
		let ethereum_storage_schema = EthereumStorageSchema::V3;

		let block_entries = vec![
			// Block 1
			(
				1i32,
				ethereum_hash_1,
				substrate_hash_1,
				ethereum_storage_schema,
			),
			// Block 2
			(
				2i32,
				ethereum_hash_2,
				substrate_hash_2,
				ethereum_storage_schema,
			),
			// Block 3
			(
				3i32,
				ethereum_hash_3,
				substrate_hash_3,
				ethereum_storage_schema,
			),
		];
		let mut builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
			"INSERT INTO blocks(
				block_number,
				ethereum_block_hash,
				substrate_block_hash,
				ethereum_storage_schema,
				is_canon)",
		);
		builder.push_values(block_entries, |mut b, entry| {
			let block_number = entry.0;
			let ethereum_block_hash = entry.1.as_bytes().to_owned();
			let substrate_block_hash = entry.2.as_bytes().to_owned();
			let ethereum_storage_schema = entry.3.encode();

			b.push_bind(block_number);
			b.push_bind(ethereum_block_hash);
			b.push_bind(substrate_block_hash);
			b.push_bind(ethereum_storage_schema);
			b.push_bind(1i32);
		});
		let query = builder.build();
		let _ = query.execute(indexer_backend.pool()).await;

		let log_entries = vec![
			// Block 1
			(
				alice,
				topics_a,
				topics_b,
				topics_c,
				topics_d,
				0,
				0,
				substrate_hash_1,
			),
			(
				alice,
				topics_d,
				topics_c,
				topics_b,
				topics_a,
				1,
				0,
				substrate_hash_1,
			),
			(
				alice,
				topics_b,
				topics_a,
				topics_d,
				topics_c,
				2,
				0,
				substrate_hash_1,
			),
			// Block 2
			(
				bob,
				topics_a,
				topics_b,
				topics_c,
				topics_d,
				0,
				0,
				substrate_hash_2,
			),
			(
				bob,
				topics_d,
				topics_c,
				topics_b,
				topics_a,
				1,
				0,
				substrate_hash_2,
			),
			(
				bob,
				topics_b,
				topics_a,
				topics_d,
				topics_c,
				2,
				0,
				substrate_hash_2,
			),
			// Block 3
			(
				bob,
				topics_a,
				topics_b,
				topics_c,
				topics_d,
				0,
				0,
				substrate_hash_3,
			),
			(
				bob,
				topics_d,
				topics_c,
				topics_b,
				topics_a,
				1,
				0,
				substrate_hash_3,
			),
			(
				bob,
				topics_b,
				topics_a,
				topics_d,
				topics_c,
				2,
				0,
				substrate_hash_3,
			),
		];

		let mut builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
			"INSERT INTO logs(
				address,
				topic_1,
				topic_2,
				topic_3,
				topic_4,
				log_index,
				transaction_index,
				substrate_block_hash)",
		);
		builder.push_values(log_entries, |mut b, entry| {
			let address = entry.0.as_bytes().to_owned();
			let topic_1 = entry.1.as_bytes().to_owned();
			let topic_2 = entry.2.as_bytes().to_owned();
			let topic_3 = entry.3.as_bytes().to_owned();
			let topic_4 = entry.4.as_bytes().to_owned();
			let log_index = entry.5;
			let transaction_index = entry.6;
			let substrate_block_hash = entry.7.as_bytes().to_owned();

			b.push_bind(address);
			b.push_bind(topic_1);
			b.push_bind(topic_2);
			b.push_bind(topic_3);
			b.push_bind(topic_4);
			b.push_bind(log_index);
			b.push_bind(transaction_index);
			b.push_bind(substrate_block_hash);
		});
		let query = builder.build();
		let _ = query.execute(indexer_backend.pool()).await;

		TestData {
			alice,
			bob,
			topics_a,
			topics_b,
			topics_c,
			topics_d,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_1,
			ethereum_hash_2,
			ethereum_hash_3,
			backend: indexer_backend,
		}
	}

	async fn run_test_case(
		backend: super::Backend<OpaqueBlock>,
		test_case: &TestFilter,
	) -> Result<Vec<FilteredLog>, String> {
		backend
			.filter_logs(
				test_case.from_block,
				test_case.to_block,
				test_case.addresses.clone(),
				test_case.topics.clone(),
			)
			.await
	}

	#[tokio::test]
	async fn genesis_works() {
		let TestData { backend, .. } = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 0,
			addresses: vec![],
			topics: vec![],
			expected_result: vec![],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn unsanitized_input_works() {
		let TestData { backend, .. } = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 0,
			addresses: vec![],
			topics: vec![vec![None], vec![None, None, None]],
			expected_result: vec![],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn invalid_topic_input_size_fails() {
		let TestData {
			backend, topics_a, ..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 0,
			addresses: vec![],
			topics: vec![
				vec![Some(topics_a), None, None, None, None],
				vec![Some(topics_a), None, None, None],
			],
			expected_result: vec![],
		};
		let _result = run_test_case(backend, &filter)
			.await
			.expect_err("Invalid topic input. Maximum length is 4.");
	}

	#[tokio::test]
	async fn malformed_topic_product_does_not_panic() {
		let TestData {
			backend, topics_a, ..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 0,
			addresses: vec![],
			topics: vec![
				vec![Some(topics_a), None, Some(topics_a)],
				vec![None],
				vec![Some(topics_a), Some(topics_a)],
			],
			expected_result: vec![],
		};
		let _result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
	}

	#[tokio::test]
	async fn block_range_works() {
		let TestData {
			backend,
			substrate_hash_1,
			substrate_hash_2,
			ethereum_hash_1,
			ethereum_hash_2,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 2,
			addresses: vec![],
			topics: vec![],
			expected_result: vec![
				(substrate_hash_1, ethereum_hash_1, 1, 0, 0).into(),
				(substrate_hash_1, ethereum_hash_1, 1, 0, 1).into(),
				(substrate_hash_1, ethereum_hash_1, 1, 0, 2).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 0).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 1).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 2).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn address_filter_works() {
		let TestData {
			backend,
			alice,
			substrate_hash_1,
			ethereum_hash_1,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice],
			topics: vec![],
			expected_result: vec![
				(substrate_hash_1, ethereum_hash_1, 1, 0, 0).into(),
				(substrate_hash_1, ethereum_hash_1, 1, 0, 1).into(),
				(substrate_hash_1, ethereum_hash_1, 1, 0, 2).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn topic_filter_works() {
		let TestData {
			backend,
			topics_d,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_1,
			ethereum_hash_2,
			ethereum_hash_3,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![],
			topics: vec![vec![Some(topics_d)]],
			expected_result: vec![
				(substrate_hash_1, ethereum_hash_1, 1, 0, 1).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 1).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 1).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	// Test filter that includes one address and one topic.
	async fn multi_filter_one_one_works() {
		let TestData {
			backend,
			bob,
			topics_b,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_2,
			ethereum_hash_3,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![bob],
			topics: vec![vec![Some(topics_b)]],
			expected_result: vec![
				(substrate_hash_2, ethereum_hash_2, 2, 0, 2).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 2).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	// Test filter that includes many addresses and one topic.
	async fn multi_filter_many_one_works() {
		let TestData {
			backend,
			alice,
			bob,
			topics_b,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_1,
			ethereum_hash_2,
			ethereum_hash_3,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice, bob],
			topics: vec![vec![Some(topics_b)]],
			expected_result: vec![
				(substrate_hash_1, ethereum_hash_1, 1, 0, 2).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 2).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 2).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	// Test filter that includes many addresses and many topics.
	async fn multi_filter_many_many_works() {
		let TestData {
			backend,
			alice,
			bob,
			topics_a,
			topics_b,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_1,
			ethereum_hash_2,
			ethereum_hash_3,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice, bob],
			topics: vec![vec![Some(topics_a), Some(topics_b)]],
			expected_result: vec![
				(substrate_hash_1, ethereum_hash_1, 1, 0, 0).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 0).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 0).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	// Test filter that includes topic wildcards.
	async fn filter_with_wildcards_works() {
		let TestData {
			backend,
			alice,
			bob,
			topics_d,
			topics_b,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_1,
			ethereum_hash_2,
			ethereum_hash_3,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice, bob],
			topics: vec![vec![Some(topics_d), None, Some(topics_b)]],
			expected_result: vec![
				(substrate_hash_1, ethereum_hash_1, 1, 0, 1).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 1).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 1).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn trailing_wildcard_is_useless_but_works() {
		let TestData {
			alice,
			backend,
			topics_b,
			substrate_hash_1,
			ethereum_hash_1,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 1,
			addresses: vec![alice],
			topics: vec![vec![None, None, Some(topics_b), None]],
			expected_result: vec![(substrate_hash_1, ethereum_hash_1, 1, 0, 1).into()],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	// Test filter that includes topic subsets.
	async fn filter_with_multiple_topic_subsets_works() {
		let TestData {
			backend,
			topics_a,
			topics_d,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_1,
			ethereum_hash_2,
			ethereum_hash_3,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![],
			topics: vec![
				vec![Some(topics_a)],
				vec![Some(topics_d)],
				vec![Some(topics_d)],
			],
			expected_result: vec![
				(substrate_hash_1, ethereum_hash_1, 1, 0, 0).into(),
				(substrate_hash_1, ethereum_hash_1, 1, 0, 1).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 0).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 1).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 0).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 1).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	// Test filter that includes topic subsets and wildcards.
	async fn filter_with_multiple_topic_subsets_and_wildcards_works() {
		let TestData {
			backend,
			bob,
			topics_a,
			topics_b,
			topics_c,
			topics_d,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_2,
			ethereum_hash_3,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![bob],
			// Product on input [null,null,(b,d),(a,c)].
			topics: vec![
				vec![None, None, Some(topics_b), Some(topics_a)],
				vec![None, None, Some(topics_b), Some(topics_c)],
				vec![None, None, Some(topics_d), Some(topics_a)],
				vec![None, None, Some(topics_d), Some(topics_c)],
			],
			expected_result: vec![
				(substrate_hash_2, ethereum_hash_2, 2, 0, 1).into(),
				(substrate_hash_2, ethereum_hash_2, 2, 0, 2).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 1).into(),
				(substrate_hash_3, ethereum_hash_3, 3, 0, 2).into(),
			],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
	}

	#[test]
	fn test_query_should_be_generated_correctly() {
		use sqlx::{Execute, Sqlite};

		let from_block: u64 = 100;
		let to_block: u64 = 500;
		let addresses: Vec<H160> = vec![
			H160::from_str("0x75e76b29a6c48f6e1aeeda4e52d3d4fa6e9355c0").unwrap(),
			H160::from_str("0x42b15a11cc295be6f97aa4518e850b064b64fb11").unwrap(),
			H160::from_str("0xf02d804b19b0665690f6b312691b2eb8f80cd3b8").unwrap(),
		];
		let topics: Vec<Vec<Option<H256>>> = vec![
			vec![
				Some(
					H256::from_str(
						"0x4dec04e750ca11537cabcd8a9eab06494de08da3735bc8871cd41250e190bc04",
					)
					.unwrap(),
				),
				Some(
					H256::from_str(
						"0x2caecd17d02f56fa897705dcc740da2d237c373f70686f4e0d9bd3bf0400ea7a",
					)
					.unwrap(),
				),
			],
			vec![
				None, // None should be omitted, leaving only a single value here
				Some(
					H256::from_str(
						"0x00000000000000000000000081e42baeee09de2880d4a8842edb1911755ac48d",
					)
					.unwrap(),
				),
				None,
			],
			vec![
				None, // topic_3 should be omitted from query
			],
			vec![Some(
				H256::from_str(
					"0x000000000000000000000000aa342baeee09de2880d4a8842edb1911755ac123",
				)
				.unwrap(),
			)],
		];

		let expected_query_sql = "
SELECT
	l.substrate_block_hash,
	b.ethereum_block_hash,
	b.block_number,
	b.ethereum_storage_schema,
	l.transaction_index,
	l.log_index
FROM logs AS l
INNER JOIN blocks AS b
ON (b.block_number BETWEEN ? AND ?) AND b.substrate_block_hash = l.substrate_block_hash AND b.is_canon = 1
WHERE l.address IN (?, ?, ?) AND l.topic_1 IN (?, ?) AND l.topic_2 = ? AND l.topic_4 = ?
GROUP BY l.substrate_block_hash, l.transaction_index, l.log_index
ORDER BY l.block_number ASC, l.transaction_index ASC, l.log_index ASC
LIMIT 10001";

		let mut qb = QueryBuilder::new("");
		let actual_query_sql =
			super::build_sqlite_query(&mut qb, from_block, to_block, addresses, topics).sql();
		assert_eq!(expected_query_sql, actual_query_sql);
	}
}
