// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2021 Parity Technologies (UK) Ltd.
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

mod utils;

pub use sp_database::Database;

use std::{sync::Arc, path::{Path, PathBuf}, marker::PhantomData};
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use parking_lot::Mutex;
use codec::{Encode, Decode};

const DB_HASH_LEN: usize = 32;
/// Hash type that this backend uses for the database.
pub type DbHash = [u8; DB_HASH_LEN];

/// Database settings.
pub struct DatabaseSettings {
	/// Where to find the database.
	pub source: DatabaseSettingsSrc,
}

/// Where to find the database.
#[derive(Debug, Clone)]
pub enum DatabaseSettingsSrc {
	/// Load a RocksDB database from a given path. Recommended for most uses.
	RocksDb {
		/// Path to the database.
		path: PathBuf,
		/// Cache size in MiB.
		cache_size: usize,
	},
}

impl DatabaseSettingsSrc {
	/// Return dabase path for databases that are on the disk.
	pub fn path(&self) -> Option<&Path> {
		match self {
			DatabaseSettingsSrc::RocksDb { path, .. } => Some(path.as_path()),
		}
	}
}

pub(crate) mod columns {
	pub const NUM_COLUMNS: u32 = 3;

	pub const META: u32 = 0;
	pub const BLOCK_MAPPING: u32 = 1;
	pub const TRANSACTION_MAPPING: u32 = 2;
}

pub struct Backend<Block: BlockT> {
	mapping_db: Arc<MappingDb<Block>>,
}

impl<Block: BlockT> Backend<Block> {
	pub fn new(config: &DatabaseSettings) -> Result<Self, String> {
		let db = utils::open_database(config)?;

		Ok(Self {
			mapping_db: Arc::new(MappingDb {
				db: db.clone(),
				write_lock: Arc::new(Mutex::new(())),
				_marker: PhantomData,
			})
		})
	}

	pub fn mapping_db(&self) -> &Arc<MappingDb<Block>> {
		&self.mapping_db
	}
}

pub struct MappingCommitment<Block: BlockT> {
	pub block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub ethereum_transaction_hashes: Vec<H256>,
}

#[derive(Clone, Encode, Decode)]
pub struct TransactionMetadata<Block: BlockT> {
	pub block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub ethereum_index: u32,
}

pub struct MappingDb<Block: BlockT> {
	db: Arc<dyn Database<DbHash>>,
	write_lock: Arc<Mutex<()>>,
	_marker: PhantomData<Block>,
}

impl<Block: BlockT> MappingDb<Block> {
	pub fn block_hashes(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Vec<Block::Hash>, String> {
		match self.db.get(crate::columns::BLOCK_MAPPING, &ethereum_block_hash.encode()) {
			Some(raw) => Ok(Vec::<Block::Hash>::decode(&mut &raw[..]).map_err(|e| format!("{:?}", e))?),
			None => Ok(Vec::new()),
		}
	}

	pub fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String> {
		match self.db.get(crate::columns::TRANSACTION_MAPPING, &ethereum_transaction_hash.encode()) {
			Some(raw) => Ok(Vec::<TransactionMetadata<Block>>::decode(&mut &raw[..]).map_err(|e| format!("{:?}", e))?),
			None => Ok(Vec::new()),
		}
	}

	pub fn write_hashes(
		&self,
		commitment: MappingCommitment<Block>,
	) -> Result<(), String> {
		let _lock = self.write_lock.lock();

		let mut transaction = sp_database::Transaction::new();

		let mut block_hashes = self.block_hashes(&commitment.ethereum_block_hash)?;
		block_hashes.push(commitment.block_hash);
		transaction.set(
			crate::columns::BLOCK_MAPPING,
			&commitment.ethereum_block_hash.encode(),
			&block_hashes.encode()
		);

		for (i, ethereum_transaction_hash) in commitment.ethereum_transaction_hashes.into_iter().enumerate() {
			let mut metadata = self.transaction_metadata(&ethereum_transaction_hash)?;
			metadata.push(TransactionMetadata::<Block> {
				block_hash: commitment.block_hash,
				ethereum_block_hash: commitment.ethereum_block_hash,
				ethereum_index: i as u32,
			});
			transaction.set(
				crate::columns::TRANSACTION_MAPPING,
				&ethereum_transaction_hash.encode(),
				&metadata.encode(),
			);
		}

		self.db.commit(transaction).map_err(|e| format!("{:?}", e))?;

		Ok(())
	}
}
