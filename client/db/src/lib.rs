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
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;

pub mod kv;
pub mod sql;
use kv::{columns, static_keys};

#[derive(Clone)]
pub enum Backend<Block: BlockT> {
	KeyValue(kv::Backend<Block>),
	Sql(sql::Backend<Block>),
}

#[derive(Clone, Encode, Debug, Decode, PartialEq)]
pub struct TransactionMetadata<Block: BlockT> {
	pub block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub ethereum_index: u32,
}

pub trait BackendReader<Block: BlockT> {
	fn block_hash(&self, ethereum_block_hash: &H256) -> Result<Option<Vec<Block::Hash>>, String>;
	fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String>;
}
