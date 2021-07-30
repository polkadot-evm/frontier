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

use std::{fs, sync::Arc, path::{Path, PathBuf}, io::{Read, Write, ErrorKind}};
use crate::{Database, DbHash, DatabaseSettings, DatabaseSettingsSrc};

/// Version file name.
const VERSION_FILE_NAME: &'static str = "db_version";
/// Current db version.
const CURRENT_VERSION: u32 = 2;

pub fn upgrade_db(db_path: &Path) -> Result<(), String> {
	let is_empty = db_path.read_dir().map_or(true, |mut d| d.next().is_none());
	if !is_empty {
		let db_version = current_version(db_path)?;
		match db_version {
			0 => Err(format!("Unsupported database version: {}", db_version))?,
			1 => {
				migrate_1_to_2(db_path)?
			},
			CURRENT_VERSION => (),
			_ => Err(format!("Future database version: {}", db_version))?,
		}
	}

	update_version(db_path, None)
}

/// Migration from version1 to version2:
/// 1) the number of columns has changed from 4 to 5;
/// 2) ETHEREUM_SCHEMA_CACHE column is added;
fn migrate_1_to_2(db_path: &Path) -> Result<(), String> {
	let db_path = db_path.to_str()
		.ok_or_else(|| "Invalid database path")?;
	let db_cfg = kvdb_rocksdb::DatabaseConfig::with_columns(crate::columns::V1_NUM_COLUMNS);
	let db = kvdb_rocksdb::Database::open(&db_cfg, db_path).map_err(|err| format!("{}", err))?;
	db.add_column().map_err(|err| format!("{}", err))
}

/// Reads current database version from the file at given path.
/// If the file does not exist returns 0.
fn current_version(path: &Path) -> Result<u32, String> {
	let unknown_version_err = || "Unknown database version".into();

	match fs::File::open(version_file_path(path)) {
		Err(ref err) if err.kind() == ErrorKind::NotFound => Ok(0),
		Err(_) => Err(unknown_version_err()),
		Ok(mut file) => {
			let mut s = String::new();
			file.read_to_string(&mut s).map_err(|_| unknown_version_err())?;
			u32::from_str_radix(&s, 10).map_err(|_| unknown_version_err())
		},
	}
}

/// Returns the version file path.
fn version_file_path(path: &Path) -> PathBuf {
	let mut file_path = path.to_owned();
	file_path.push(VERSION_FILE_NAME);
	file_path
}

/// Writes current database version to the file.
/// Creates a new file if the version file does not exist yet.
fn update_version(path: &Path, version: Option<u32>) -> Result<(), String> {
	let v = version.unwrap_or(CURRENT_VERSION);
	fs::create_dir_all(path).map_err(|err| format!("{}", err))?;
	let mut file = fs::File::create(version_file_path(path)).map_err(|err| format!("{}", err))?;
	file.write_all(format!("{}", v).as_bytes()).map_err(|err| format!("{}", err))?;
	Ok(())
}

pub fn open_database(
	config: &DatabaseSettings,
) -> Result<Arc<dyn Database<DbHash>>, String> {
	let db: Arc<dyn Database<DbHash>> = match &config.source {
		DatabaseSettingsSrc::RocksDb { path, cache_size: _ } => {
			// We introduce versioning as part of the V2.
			// Make sure that migration happens on a versionless client upgrade to V2.
			if !version_file_path(&path).exists() && CURRENT_VERSION == 2 {
				update_version(&path, Some(1))?;
			}
			// Upgrade database to required version.
			upgrade_db(&path)?;

			let db_config = kvdb_rocksdb::DatabaseConfig::with_columns(crate::columns::NUM_COLUMNS);
			let path = path.to_str()
				.ok_or_else(|| "Invalid database path".to_string())?;

			let db = kvdb_rocksdb::Database::open(&db_config, &path)
				.map_err(|err| format!("{}", err))?;
			sp_database::as_database(db)
		}
	};

	Ok(db)
}
