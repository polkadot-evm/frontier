// This file is part of Tokfin.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![warn(unused_crate_dependencies)]

use std::sync::Arc;

// Substrate
pub use sc_client_db::DatabaseSource;

pub mod kv;
#[cfg(feature = "sql")]
pub mod sql;

#[derive(Clone)]
pub enum Backend<Block, C> {
	KeyValue(Arc<kv::Backend<Block, C>>),
	#[cfg(feature = "sql")]
	Sql(Arc<sql::Backend<Block>>),
}
