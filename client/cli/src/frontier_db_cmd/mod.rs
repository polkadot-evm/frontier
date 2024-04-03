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

mod mapping_db;
mod meta_db;
#[cfg(test)]
mod tests;
pub(crate) mod utils;

use std::{path::PathBuf, str::FromStr, sync::Arc};

use clap::ValueEnum;
use ethereum_types::H256;
use serde::Deserialize;
// Substrate
use sc_cli::{PruningParams, SharedParams};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;

use self::{
	mapping_db::{MappingDb, MappingKey, MappingValue},
	meta_db::{MetaDb, MetaKey, MetaValue},
};

/// Cli tool to interact with the Frontier backend db
#[derive(Debug, Clone, clap::Parser)]
pub struct FrontierDbCmd {
	/// Specify the operation to perform.
	///
	/// Can be one of `create | read | update | delete`.
	#[arg(value_enum, ignore_case = true, required = true)]
	pub operation: Operation,

	/// Specify the column to query.
	///
	/// Can be one of `meta | block | transaction`.
	#[arg(value_enum, ignore_case = true, required = true)]
	pub column: Column,

	/// Specify the key to either read or write.
	#[arg(short('k'), long, required = true)]
	pub key: String,

	/// Specify the value to write.
	///
	/// - When `Some`, path to file.
	/// - When `None`, read from stdin.
	///
	/// In any case, payload must be serializable to a known type.
	#[arg(long)]
	pub value: Option<PathBuf>,

	/// Shared parameters
	#[command(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[command(flatten)]
	pub pruning_params: PruningParams,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum Operation {
	Create,
	Read,
	Update,
	Delete,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum Column {
	Meta,
	Block,
	Transaction,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DbValue<H> {
	Meta(MetaValue<H>),
	Mapping(MappingValue<H>),
}

impl FrontierDbCmd {
	pub fn run<C, B: BlockT>(
		&self,
		client: Arc<C>,
		backend: Arc<fc_db::kv::Backend<B>>,
	) -> sc_cli::Result<()>
	where
		C: ProvideRuntimeApi<B>,
		C::Api: fp_rpc::EthereumRuntimeRPCApi<B>,
		C: HeaderBackend<B>,
	{
		match self.column {
			Column::Meta => {
				// New meta db handler
				let meta_db = MetaDb::new(self, backend);
				// Maybe get a MetaKey
				let key = MetaKey::from_str(&self.key)?;
				// Maybe get a MetaValue
				let value = match utils::maybe_deserialize_value::<B>(
					&self.operation,
					self.value.as_ref(),
				)? {
					Some(DbValue::Meta(value)) => Some(value),
					None => None,
					_ => return Err(format!("Unexpected `{:?}` value", self.value).into()),
				};
				// Run the query
				meta_db.query(&key, &value)?
			}
			Column::Block | Column::Transaction => {
				// New mapping db handler
				let mapping_db = MappingDb::new(self, client, backend);
				// Maybe get a MappingKey
				let key = MappingKey::EthBlockOrTransactionHash(
					H256::from_str(&self.key).expect("H256 provided key"),
				);
				// Maybe get a MappingValue
				let value = match utils::maybe_deserialize_value::<B>(
					&self.operation,
					self.value.as_ref(),
				)? {
					Some(DbValue::Mapping(value)) => Some(value),
					None => None,
					_ => return Err(format!("Unexpected `{:?}` value", self.value).into()),
				};
				// Run the query
				mapping_db.query(&self.column, &key, &value)?
			}
		}
		Ok(())
	}
}

impl sc_cli::CliConfiguration for FrontierDbCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}

	fn pruning_params(&self) -> Option<&PruningParams> {
		Some(&self.pruning_params)
	}
}
