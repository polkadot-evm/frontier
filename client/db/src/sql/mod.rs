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

use codec::{Decode, Encode};
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
	sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteQueryResult},
	ConnectOptions, Error, Execute, QueryBuilder, Row, Sqlite,
};
use std::{str::FromStr, sync::Arc};

use crate::FilteredLog;

#[derive(Debug, Eq, PartialEq)]
pub struct Log {
	pub block_number: i32,
	pub address: Vec<u8>,
	pub topic_1: Vec<u8>,
	pub topic_2: Vec<u8>,
	pub topic_3: Vec<u8>,
	pub topic_4: Vec<u8>,
	pub log_index: i32,
	pub transaction_index: i32,
	pub substrate_block_hash: Vec<u8>,
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
}
impl<Block: BlockT> Backend<Block>
where
	Block: BlockT<Hash = H256> + Send + Sync,
{
	pub async fn new(
		config: BackendConfig<'_>,
		pool_size: u32,
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
		})
	}

	fn connect_options(config: &BackendConfig) -> Result<SqliteConnectOptions, Error> {
		match config {
			BackendConfig::Sqlite(config) => {
				let config = sqlx::sqlite::SqliteConnectOptions::from_str(config.path)?
					.create_if_missing(config.create_if_missing);
				Ok(config)
			}
		}
	}

	pub fn pool(&self) -> &SqlitePool {
		&self.pool
	}

	pub async fn insert_genesis_block_metadata<Client, BE>(
		&self,
		client: Arc<Client>,
	) -> Result<(), Error>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		Client: ProvideRuntimeApi<Block>,
		Client::Api: EthereumRuntimeRPCApi<Block>,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let id = BlockId::Number(Zero::zero());
		if let Ok(Some(genesis_header)) = client.header(id) {
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

				let ethereum_block_hash = ethereum_block.header.hash().as_bytes().to_owned();
				let substrate_block_hash = genesis_header.hash().as_bytes().to_owned();
				let schema = Self::onchain_storage_schema(client.as_ref(), id).encode();

				let _ = sqlx::query!(
					"INSERT OR IGNORE INTO blocks(
						ethereum_block_hash,
						substrate_block_hash,
						ethereum_storage_schema)
					VALUES (?, ?, ?)",
					ethereum_block_hash,
					substrate_block_hash,
					schema,
				)
				.execute(self.pool())
				.await?;
			}
		}
		Ok(())
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
		let mut tx = self.pool().begin().await?;

		// TODO move header retrieval to the blocking thread? depending on the batch size its likely to be necessary
		for &hash in hashes.iter() {
			if let Ok(Some(header)) = client.header(BlockId::Hash(hash)) {
				match fp_consensus::find_log(header.digest()) {
					Ok(log) => {
						let post_hashes = log.into_hashes();
						let ethereum_block_hash = post_hashes.block_hash.as_bytes().to_owned();
						let substrate_block_hash = header.hash().as_bytes().to_owned();

						let id = BlockId::Hash(header.hash());
						let schema = Self::onchain_storage_schema(client.as_ref(), id).encode();

						let _ = sqlx::query!(
							"INSERT OR IGNORE INTO blocks(
								ethereum_block_hash,
								substrate_block_hash,
								ethereum_storage_schema)
							VALUES (?, ?, ?)",
							ethereum_block_hash,
							substrate_block_hash,
							schema,
						)
						.execute(&mut tx)
						.await?;
						for (i, &transaction_hash) in
							post_hashes.transaction_hashes.iter().enumerate()
						{
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
					Err(FindLogError::NotFound) => {}
					Err(FindLogError::MultipleLogs) => {
						return Err(Error::Protocol("Multiple logs found".to_string()))
					}
				}
			}
		}

		let mut builder: QueryBuilder<Sqlite> =
			QueryBuilder::new("INSERT INTO sync_status(substrate_block_hash) ");
		builder.push_values(hashes, |mut b, hash| {
			b.push_bind(hash.as_bytes());
		});
		let query = builder.build();
		query.execute(&mut tx).await?;

		tx.commit().await
	}

	pub fn spawn_logs_task<Client, BE>(&self, client: Arc<Client>, batch_size: usize)
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let pool = self.pool().clone();
		let overrides = self.overrides.clone();
		tokio::task::spawn(async move {
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
									block_number,
									address,
									topic_1,
									topic_2,
									topic_3,
									topic_4,
									log_index,
									transaction_index,
									substrate_block_hash)
								VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
								log.block_number,
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
		});
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
		for substrate_block_hash in hashes.iter() {
			let substrate_block_number: i32 =
				if let Ok(Some(number)) = client.number(*substrate_block_hash) {
					UniqueSaturatedInto::<u32>::unique_saturated_into(number) as i32
				} else {
					log::error!(
						target: "frontier-sql",
						"Cannot find number for substrate hash {}",
						substrate_block_hash
					);
					0i32
				};
			let id = BlockId::Hash(*substrate_block_hash);
			let schema = Self::onchain_storage_schema(client.as_ref(), id);
			let handler = overrides
				.schemas
				.get(&schema)
				.unwrap_or(&overrides.fallback);

			let receipts = handler.current_receipts(&id).unwrap_or_default();

			for (transaction_index, receipt) in receipts.iter().enumerate() {
				let receipt_logs = match receipt {
					ethereum::ReceiptV3::Legacy(d)
					| ethereum::ReceiptV3::EIP2930(d)
					| ethereum::ReceiptV3::EIP1559(d) => &d.logs,
				};
				let transaction_index = transaction_index as i32;
				for (log_index, log) in receipt_logs.iter().enumerate() {
					logs.push(Log {
						block_number: substrate_block_number,
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
		logs
	}

	fn onchain_storage_schema<Client, BE>(
		client: &Client,
		at: BlockId<Block>,
	) -> EthereumStorageSchema
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		match client.storage(
			&at,
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
				block_number INTEGER NOT NULL,
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
				ethereum_block_hash BLOB NOT NULL,
				substrate_block_hash BLOB NOT NULL,
				ethereum_storage_schema BLOB NOT NULL,
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
			CREATE INDEX IF NOT EXISTS block_number_idx ON logs (
				block_number,
				address
			);
			CREATE INDEX IF NOT EXISTS topic_1_idx ON logs (
				block_number,
				topic_1
			);
			CREATE INDEX IF NOT EXISTS topic_2_idx ON logs (
				block_number,
				topic_2
			);
			CREATE INDEX IF NOT EXISTS topic_3_idx ON logs (
				block_number,
				topic_3
			);
			CREATE INDEX IF NOT EXISTS topic_4_idx ON logs (
				block_number,
				topic_4
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
		.execute(pool)
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
	// Build sql query from rpc filter data
	async fn filter_logs(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Vec<H160>,
		topics: Vec<Vec<Option<H256>>>,
	) -> Result<Vec<FilteredLog>, String> {
		// Sanitize topic input
		let mut topics = topics;
		topics.retain(|topic_group| !topic_group.iter().all(|x| x.is_none()));

		let filter_groups: Vec<Vec<FilterValue>> = match (addresses.len(), topics.len()) {
			(x, 0) if x > 0 => addresses
				.iter()
				.map(|address| vec![FilterValue::Address(*address)])
				.collect(),
			(0, y) if y > 0 => topics
				.iter()
				.map(|topic_group| {
					topic_group
						.iter()
						.map(|topic| FilterValue::Topic(*topic))
						.collect()
				})
				.collect(),
			(_, _) => {
				let mut out = vec![];
				for address in addresses.iter() {
					for topic_group in topics.iter() {
						let mut inner = vec![FilterValue::Address(*address)];
						for topic in topic_group.iter() {
							inner.push(FilterValue::Topic(*topic));
						}
						out.push(inner);
					}
				}
				out
			}
		};
		let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
			"
			SELECT
				A.substrate_block_hash,
				B.ethereum_block_hash,
				A.block_number,
				B.ethereum_storage_schema,
				A.transaction_index,
				A.log_index
			FROM logs AS A
			INNER JOIN blocks AS B
			ON A.substrate_block_hash = B.substrate_block_hash
			WHERE A.block_number BETWEEN ",
		);
		// Bind `from` and `to` block range
		let mut block_number = query_builder.separated(" AND ");
		block_number.push_bind(from_block as i64);
		block_number.push_bind(to_block as i64);
		// Address and topics substatement
		if !filter_groups.is_empty() {
			query_builder.push(" AND (");
		}
		for (i, filter_group) in filter_groups.iter().enumerate() {
			query_builder.push("(");
			let mut topic_pos = 1;
			for (j, el) in filter_group.iter().enumerate() {
				let mut add_separator = false;
				match el {
					FilterValue::Address(address) => {
						query_builder.push("address = ");
						let address = address.as_bytes().to_owned();
						query_builder.push_bind(address);
						add_separator = true;
					}
					FilterValue::Topic(topic) => {
						if let Some(topic) = topic {
							let topic = topic.as_bytes().to_owned();
							match topic_pos {
								1 => {
									query_builder.push("topic_1 = ");
									query_builder.push_bind(topic);
								}
								2 => {
									query_builder.push("topic_2 = ");
									query_builder.push_bind(topic);
								}
								3 => {
									query_builder.push("topic_3 = ");
									query_builder.push_bind(topic);
								}
								4 => {
									query_builder.push("topic_4 = ");
									query_builder.push_bind(topic);
								}
								_ => todo!(),
							}
							add_separator = true;
						}
						topic_pos += 1;
					}
				}
				if add_separator && j < filter_group.len() - 1 {
					query_builder.push(" AND ");
				}
			}
			query_builder.push(")");
			if i < filter_groups.len() - 1 {
				query_builder.push(" OR ");
			}
		}
		if !filter_groups.is_empty() {
			query_builder.push(")");
		}
		query_builder.push(
			"
			GROUP BY A.substrate_block_hash, transaction_index, log_index
			ORDER BY block_number ASC, transaction_index ASC, log_index ASC
		",
		);

		let query = query_builder.build();
		let sql = query.sql();

		let mut out: Vec<FilteredLog> = vec![];
		match query.fetch_all(self.pool()).await {
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
			_ => {
				log::error!(
					target: "frontier-sql",
					"Failed to query sql db with statement {:?}",
					sql
				);
				return Err("Failed to query sql db with statement".to_string());
			}
		};

		Ok(out)
	}

	fn is_indexed(&self) -> bool {
		true
	}
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
	use std::{collections::BTreeMap, path::Path, sync::Arc};
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
			(ethereum_hash_1, substrate_hash_1, ethereum_storage_schema),
			// Block 2
			(ethereum_hash_2, substrate_hash_2, ethereum_storage_schema),
			// Block 3
			(ethereum_hash_3, substrate_hash_3, ethereum_storage_schema),
		];
		let mut builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
			"INSERT INTO blocks(
				ethereum_block_hash,
				substrate_block_hash,
				ethereum_storage_schema)",
		);
		builder.push_values(block_entries, |mut b, entry| {
			let ethereum_block_hash = entry.0.as_bytes().to_owned();
			let substrate_block_hash = entry.1.as_bytes().to_owned();
			let ethereum_storage_schema = entry.2.encode();

			b.push_bind(ethereum_block_hash);
			b.push_bind(substrate_block_hash);
			b.push_bind(ethereum_storage_schema);
		});
		let query = builder.build();
		let _ = query.execute(indexer_backend.pool()).await;

		let log_entries = vec![
			// Block 1
			(
				1,
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
				1,
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
				1,
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
				2,
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
				2,
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
				2,
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
				3,
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
				3,
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
				3,
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
				block_number,
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
			let block_number = entry.0;
			let address = entry.1.as_bytes().to_owned();
			let topic_1 = entry.2.as_bytes().to_owned();
			let topic_2 = entry.3.as_bytes().to_owned();
			let topic_3 = entry.4.as_bytes().to_owned();
			let topic_4 = entry.5.as_bytes().to_owned();
			let log_index = entry.6;
			let transaction_index = entry.7;
			let substrate_block_hash = entry.8.as_bytes().to_owned();

			b.push_bind(block_number);
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
			topics: vec![vec![None]],
			expected_result: vec![],
		};
		let result = run_test_case(backend, &filter)
			.await
			.expect("run test case");
		assert_eq!(result, filter.expected_result);
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
			ethereum_hash_1,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 1,
			addresses: vec![alice, bob],
			topics: vec![vec![Some(topics_d), None, Some(topics_b)]],
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
			topics_b,
			topics_c,
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
			topics: vec![
				vec![None, None, Some(topics_b)],
				vec![None, None, None, Some(topics_c)],
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
}
