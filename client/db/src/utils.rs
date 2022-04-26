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

use std::sync::Arc;

use crate::{Database, DatabaseSettings, DatabaseSource, DbHash};

pub fn open_database(config: &DatabaseSettings) -> Result<Arc<dyn Database<DbHash>>, String> {

	#[cfg(feature = "with-kvdb-rocksdb")]
	if let DatabaseSource::RocksDb { path, cache_size: _ } = &config.source {
		let db_config = kvdb_rocksdb::DatabaseConfig::with_columns(crate::columns::NUM_COLUMNS);
		let path = path
			.to_str()
			.ok_or_else(|| "Invalid database path".to_string())?;
		
		let db = kvdb_rocksdb::Database::open(&db_config, &path)
			.map_err(|err| format!("{}", err))?;
		return Ok(sp_database::as_database(db));
	}

	#[cfg(feature = "with-parity-db")]
	if let DatabaseSource::ParityDb { path } = &config.source {
		let config = parity_db::Options::with_columns(path, crate::columns::NUM_COLUMNS as u8);
		let db = parity_db::Db::open_or_create(&config)
			.map_err(|err| format!("{}", err))?;
		return Ok(Arc::new(crate::parity_db_adapter::DbAdapter(db)));
	}

	panic!("Cannot resolve database source or missing feature flags `with-kvdb-rocksdb` | `with-parity-db`");
}
