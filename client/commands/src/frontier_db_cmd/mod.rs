// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2021-2022 Parity Technologies (UK) Ltd.
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

mod meta_db;
mod tests;
pub(crate) mod utils;

use meta_db::{MetaDb, MetaKey, MetaValue};

use clap::ArgEnum;
use sc_cli::SharedParams;
use serde::Deserialize;
use std::{path::PathBuf, str::FromStr, sync::Arc};

use sp_runtime::traits::Block as BlockT;

/// Cli tool to interact with the Frontier backend db
#[derive(Debug, Clone, clap::Parser)]
pub struct FrontierDbCmd {
	/// Specify the operation to perform.
	///
	/// Can be one of `create | read | update | delete`.
	#[clap(arg_enum, ignore_case = true, required = true)]
	pub operation: Operation,

	/// Specify the column to query.
	///
	/// Can be one of `meta | block | transaction`.
	#[clap(arg_enum, ignore_case = true, required = true)]
	pub column: Column,

	/// Specify the key to either read or write.
	#[clap(short('k'), long, required = true)]
	pub key: String,

	/// Specify the value to write.
	///
	/// - When `Some`, path to file.
	/// - When `None`, read from stdin.
	///
	/// In any case, payload must be serializable to a known type.
	#[clap(long, parse(from_os_str))]
	pub value: Option<PathBuf>,

	/// Shared parameters
	#[clap(flatten)]
	pub shared_params: SharedParams,
}

#[derive(ArgEnum, Debug, Clone)]
pub enum Operation {
	Create,
	Read,
	Update,
	Delete,
}

#[derive(ArgEnum, Debug, Clone)]
pub enum Column {
	Meta,
	Block,
	Transaction,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DbValue<H> {
	Meta(MetaValue<H>),
	ToDo,
}

impl FrontierDbCmd {
	pub fn run<B: BlockT>(&self, backend: Arc<fc_db::Backend<B>>) -> sc_cli::Result<()> {
		match self.column {
			Column::Meta => {
				// New meta db handler
				let meta_db = MetaDb::new(&self, backend);
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
			_ => return Err(format!("`{:?}` column not supported", self.column).into()),
		}
		Ok(())
	}
}

impl sc_cli::CliConfiguration for FrontierDbCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}
}
