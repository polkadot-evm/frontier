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

mod overrides;
pub use self::overrides::*;

use scale_codec::Decode;
// Substrate
use sc_client_api::{backend::Backend, StorageProvider};
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use sp_storage::StorageKey;
// Frontier
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};

pub fn onchain_storage_schema<B: BlockT, C, BE>(client: &C, at: BlockId<B>) -> EthereumStorageSchema
where
	B: BlockT,
	C: StorageProvider<B, BE> + HeaderBackend<B> + 'static,
	BE: Backend<B>,
{
	if let Ok(Some(hash)) = client.block_hash_from_id(&at) {
		match client.storage(hash, &StorageKey(PALLET_ETHEREUM_SCHEMA.to_vec())) {
			Ok(Some(bytes)) => Decode::decode(&mut &bytes.0[..])
				.ok()
				.unwrap_or(EthereumStorageSchema::Undefined),
			_ => EthereumStorageSchema::Undefined,
		}
	} else {
		EthereumStorageSchema::Undefined
	}
}
