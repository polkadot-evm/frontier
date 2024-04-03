// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

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

use std::{path::Path, sync::Arc};

// Substrate
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;

use super::{Database, DatabaseSettings, DatabaseSource, DbHash};

pub fn open_database<Block: BlockT, C: HeaderBackend<Block>>(
	client: Arc<C>,
	config: &DatabaseSettings,
) -> Result<Arc<dyn Database<DbHash>>, String> {
	let db: Arc<dyn Database<DbHash>> = match &config.source {
		DatabaseSource::Auto {
			paritydb_path,
			rocksdb_path,
			..
		} => {
			match open_kvdb_rocksdb::<Block, C>(client.clone(), rocksdb_path, false, &config.source)
			{
				Ok(db) => db,
				Err(_) => open_parity_db::<Block, C>(client, paritydb_path, &config.source)?,
			}
		}
		#[cfg(feature = "rocksdb")]
		DatabaseSource::RocksDb { path, .. } => {
			open_kvdb_rocksdb::<Block, C>(client, path, true, &config.source)?
		}
		DatabaseSource::ParityDb { path } => {
			open_parity_db::<Block, C>(client, path, &config.source)?
		}
		_ => return Err("Supported db sources: `auto` | `rocksdb` | `paritydb`".to_string()),
	};
	Ok(db)
}

#[allow(unused_variables)]
#[cfg(feature = "rocksdb")]
fn open_kvdb_rocksdb<Block: BlockT, C: HeaderBackend<Block>>(
	client: Arc<C>,
	path: &Path,
	create: bool,
	_source: &DatabaseSource,
) -> Result<Arc<dyn Database<DbHash>>, String> {
	// first upgrade database to required version
	#[cfg(not(test))]
	match super::upgrade::upgrade_db::<Block, C>(client, path, _source) {
		Ok(_) => (),
		Err(_) => return Err("Frontier DB upgrade error".to_string()),
	}

	let mut db_config = kvdb_rocksdb::DatabaseConfig::with_columns(super::columns::NUM_COLUMNS);
	db_config.create_if_missing = create;

	let db = kvdb_rocksdb::Database::open(&db_config, path).map_err(|err| format!("{}", err))?;
	// write database version only after the database is successfully opened
	#[cfg(not(test))]
	super::upgrade::update_version(path).map_err(|_| "Cannot update db version".to_string())?;
	Ok(sp_database::as_database(db))
}

#[cfg(not(feature = "rocksdb"))]
fn open_kvdb_rocksdb<Block: BlockT, C: HeaderBackend<Block>>(
	_client: Arc<C>,
	_path: &Path,
	_create: bool,
	_source: &DatabaseSource,
) -> Result<Arc<dyn Database<DbHash>>, String> {
	Err("Missing feature flags `rocksdb`".to_string())
}

#[allow(unused_variables)]
fn open_parity_db<Block: BlockT, C: HeaderBackend<Block>>(
	client: Arc<C>,
	path: &Path,
	_source: &DatabaseSource,
) -> Result<Arc<dyn Database<DbHash>>, String> {
	// first upgrade database to required version
	#[cfg(not(test))]
	match super::upgrade::upgrade_db::<Block, C>(client, path, _source) {
		Ok(_) => (),
		Err(_) => return Err("Frontier DB upgrade error".to_string()),
	}
	let mut config = parity_db::Options::with_columns(path, super::columns::NUM_COLUMNS as u8);
	config.columns[super::columns::BLOCK_MAPPING as usize].btree_index = true;

	let db = parity_db::Db::open_or_create(&config).map_err(|err| format!("{}", err))?;
	// write database version only after the database is successfully opened
	#[cfg(not(test))]
	super::upgrade::update_version(path).map_err(|_| "Cannot update db version".to_string())?;
	Ok(Arc::new(super::parity_db_adapter::DbAdapter(db)))
}
