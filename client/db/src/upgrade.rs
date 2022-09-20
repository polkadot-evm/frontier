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

use sp_runtime::traits::Block as BlockT;
use codec::{Decode, Encode};

use crate::{Database, DatabaseSettings, DatabaseSource, DbHash};

use std::{
	fmt, fs,
	io::{self, ErrorKind, Read, Write},
	path::{Path, PathBuf},
};

/// Version file name.
const VERSION_FILE_NAME: &str = "db_version";

/// Current db version.
const CURRENT_VERSION: u32 = 2;

/// Number of columns in each version.
const _V1_NUM_COLUMNS: u32 = 4;
const V2_NUM_COLUMNS: u32 = 4;

/// Database upgrade errors.
#[derive(Debug)]
pub(crate) enum UpgradeError {
	/// Database version cannot be read from existing db_version file.
	UnknownDatabaseVersion,
	/// Missing database version file.
	MissingDatabaseVersionFile,
	/// Database version no longer supported.
	UnsupportedVersion(u32),
	/// Database version comes from future version of the client.
	FutureDatabaseVersion(u32),
	/// Common io error.
	Io(io::Error),
}

pub(crate) type UpgradeResult<T> = Result<T, UpgradeError>;

struct UpgradeVersion1To2Summary {
	pub success: u32,
	pub error: Vec<sp_core::H256>,
}

impl From<io::Error> for UpgradeError {
	fn from(err: io::Error) -> Self {
		UpgradeError::Io(err)
	}
}

impl fmt::Display for UpgradeError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			UpgradeError::UnknownDatabaseVersion => {
				write!(f, "Database version cannot be read from existing db_version file")
			},
			UpgradeError::MissingDatabaseVersionFile => write!(f, "Missing database version file"),
			UpgradeError::UnsupportedVersion(version) => {
				write!(f, "Database version no longer supported: {}", version)
			},
			UpgradeError::FutureDatabaseVersion(version) => {
				write!(f, "Database version comes from future version of the client: {}", version)
			},
			UpgradeError::Io(err) => write!(f, "Io error: {}", err),
		}
	}
}

/// Upgrade database to current version.
pub(crate) fn upgrade_db<Block: BlockT>(db_path: &Path) -> UpgradeResult<()> {
	let db_version = current_version(db_path)?;
	match db_version {
		0 => return Err(UpgradeError::UnsupportedVersion(db_version)),
		1 => {
			let summary = migrate_1_to_2::<Block>(db_path)?;
			if summary.error.len() > 0 {
				panic!("Inconsistent migration from version 1 to 2. Failed on {:?}", summary.error);
			} else {
				log::info!("✔️ Successful Frontier DB migration from version 1 to version 2 ({:?} entries).", summary.success);
			}
		},
		CURRENT_VERSION => (),
		_ => return Err(UpgradeError::FutureDatabaseVersion(db_version)),
	}
	update_version(db_path)?;
	Ok(())
}

/// Reads current database version from the file at given path.
/// If the file does not exist it gets created with version 1.
pub(crate) fn current_version(path: &Path) -> UpgradeResult<u32> {
	match fs::File::open(version_file_path(path)) {
		Err(ref err) if err.kind() == ErrorKind::NotFound => {
			fs::create_dir_all(path)?;
			let mut file = fs::File::create(version_file_path(path))?;
			file.write_all(format!("{}", 1).as_bytes())?;
			Ok(1u32)
		},
		Err(_) => Err(UpgradeError::UnknownDatabaseVersion),
		Ok(mut file) => {
			let mut s = String::new();
			file.read_to_string(&mut s).map_err(|_| UpgradeError::UnknownDatabaseVersion)?;
			u32::from_str_radix(&s, 10).map_err(|_| UpgradeError::UnknownDatabaseVersion)
		},
	}
}

/// Writes current database version to the file.
/// Creates a new file if the version file does not exist yet.
pub(crate) fn update_version(path: &Path) -> io::Result<()> {
	fs::create_dir_all(path)?;
	let mut file = fs::File::create(version_file_path(path))?;
	file.write_all(format!("{}", CURRENT_VERSION).as_bytes())?;
	Ok(())
}

/// Returns the version file path.
fn version_file_path(path: &Path) -> PathBuf {
	let mut file_path = path.to_owned();
	file_path.push(VERSION_FILE_NAME);
	file_path
}

/// Migration from version1 to version2:
/// - The format of the Ethereum<>Substrate block mapping changed to support equivocation.
/// - Migrating schema from One-to-one to One-to-many (EthHash: Vec<SubstrateHash>) relationship.
pub(crate) fn migrate_1_to_2<Block: BlockT>(db_path: &Path) -> UpgradeResult<UpgradeVersion1To2Summary> {
	log::info!("🔨 Running Frontier DB migration from version 1 to version 2. Please wait.");
	let mut res = UpgradeVersion1To2Summary {
		success: 0,
		error: vec![],
	};
	// Process a batch of hashes in a single db transaction
	let mut process_chunk = |db: &kvdb_rocksdb::Database, ethereum_hashes: &[std::boxed::Box<[u8]>]| -> UpgradeResult<()> {
		let mut transaction = db.transaction();
		for ethereum_hash in ethereum_hashes {
			if let Some(substrate_hash) = db.get(crate::columns::BLOCK_MAPPING, ethereum_hash)? {
				// Only update version1 data
				let decoded = Vec::<Block::Hash>::decode(&mut &substrate_hash[..]);
				if decoded.is_err() || decoded.unwrap().is_empty() {
					let mut hashes = Vec::new();
					hashes.push(sp_core::H256::from_slice(&substrate_hash[..]));
					transaction.put_vec(crate::columns::BLOCK_MAPPING, ethereum_hash, hashes.encode());
					res.success = res.success + 1;
				} else {
					res.error.push(sp_core::H256::from_slice(ethereum_hash));
				}
			} else {
				res.error.push(sp_core::H256::from_slice(ethereum_hash));
			}
		}
		db.write(transaction).map_err(|_| 
			io::Error::new(ErrorKind::Other, "Failed to commit on migrate_1_to_2")
		)?;
		Ok(())
	};

    let db_cfg = kvdb_rocksdb::DatabaseConfig::with_columns(V2_NUM_COLUMNS);
	let db = kvdb_rocksdb::Database::open(&db_cfg, db_path)?;

    // Get all the block hashes we need to update
	let ethereum_hashes: Vec<_> = db.iter(crate::columns::BLOCK_MAPPING).map(|entry| entry.0).collect();

    // Read and update each entry in db transaction batches
	const CHUNK_SIZE: usize = 10_000;
	let chunks = ethereum_hashes.chunks(CHUNK_SIZE);
	for chunk in chunks {
		process_chunk(&db, chunk)?;
	}
	Ok(res)
}

#[cfg(test)]
mod tests {

	use std::{collections::HashMap, path::PathBuf, sync::Arc};

	use codec::Encode;
	use sp_core::H256;
	use sp_runtime::{
		generic::{Block, BlockId, Header},
		traits::{BlakeTwo256, Block as BlockT},
	};
	use std::str::FromStr;
	use tempfile::tempdir;

	type OpaqueBlock =
		Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

	pub fn open_frontier_backend(
		path: PathBuf,
	) -> Result<Arc<crate::Backend<OpaqueBlock>>, String> {
		Ok(Arc::new(crate::Backend::<OpaqueBlock>::new(
			&crate::DatabaseSettings {
				source: sc_client_db::DatabaseSource::RocksDb {
					path,
					cache_size: 0,
				},
			},
		)?))
	}

    #[test]
	fn upgrade_1_to_2_works() {
		let tmp = tempdir().expect("create a temporary directory");
        let path = tmp.path().to_owned();
        let mut ethereum_hashes = vec![];
        let mut substrate_hashes = vec![];
        {
            // Create a temporary frontier secondary DB.
            let backend = open_frontier_backend(path.clone()).expect("a temporary db was created");
            
            // Fill the tmp db with some data
            let mut transaction = sp_database::Transaction::new();
            for n in 0..20_010 {
                let ethhash = H256::random();
				let subhash = H256::random();
                ethereum_hashes.push(ethhash);
                substrate_hashes.push(subhash);
                transaction.set(
                    crate::columns::BLOCK_MAPPING,
                    &ethhash.encode(),
                    &subhash.encode(),
                );
            }
            let _ = backend.mapping().db.commit(transaction);

        }
        // Upgrade database from version 1 to 2
        let _ = super::upgrade_db::<OpaqueBlock>(&path);

		// Check data
        let backend = open_frontier_backend(path.clone()).expect("a temporary db was created");
        for (i, original_ethereum_hash) in ethereum_hashes.iter().enumerate() {
            let entry = backend.mapping().block_hash(original_ethereum_hash).unwrap().unwrap();
			// All entries now hold a single element Vec
			assert_eq!(entry.len(), 1);
			// The Vec holds the old value
			assert_eq!(entry.first(), substrate_hashes.get(i));
        }

		// Upgrade db version file
		assert_eq!(super::update_version(&path), Ok(2u32));
	}
}
