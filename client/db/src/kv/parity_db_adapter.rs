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

// Substrate
use sp_database::{error::DatabaseError, Change, ColumnId, Database, Transaction};

fn handle_err<T>(result: parity_db::Result<T>) -> T {
	match result {
		Ok(r) => r,
		Err(e) => {
			panic!("Critical database error: {:?}", e);
		}
	}
}

pub struct DbAdapter(pub parity_db::Db);

impl<H: Clone + AsRef<[u8]>> Database<H> for DbAdapter {
	fn commit(&self, transaction: Transaction<H>) -> Result<(), DatabaseError> {
		handle_err(
			self.0
				.commit(transaction.0.into_iter().map(|change| match change {
					Change::Set(col, key, value) => (col as u8, key, Some(value)),
					Change::Remove(col, key) => (col as u8, key, None),
					_ => unimplemented!(),
				})),
		);

		Ok(())
	}

	fn get(&self, col: ColumnId, key: &[u8]) -> Option<Vec<u8>> {
		handle_err(self.0.get(col as u8, key))
	}

	fn contains(&self, col: ColumnId, key: &[u8]) -> bool {
		handle_err(self.0.get_size(col as u8, key)).is_some()
	}

	fn value_size(&self, col: ColumnId, key: &[u8]) -> Option<usize> {
		handle_err(self.0.get_size(col as u8, key)).map(|s| s as usize)
	}

	fn supports_ref_counting(&self) -> bool {
		true
	}

	fn sanitize_key(&self, key: &mut Vec<u8>) {
		let _prefix = key.drain(0..key.len() - super::DB_HASH_LEN);
	}
}
