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


#![warn(unused_crate_dependencies)]

mod overrides;
pub use self::overrides::*;

use std::{collections::BTreeMap, sync::Arc};

use scale_codec::Decode;
// Substrate
use sc_client_api::{backend::Backend, StorageProvider};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;
// Frontier
use fp_rpc::EthereumRuntimeRPCApi;
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};

pub fn overrides_handle<B, C, BE>(client: Arc<C>) -> Arc<OverrideHandle<B>>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
{
	let mut overrides_map = BTreeMap::new();
	overrides_map.insert(
		EthereumStorageSchema::V1,
		Box::new(SchemaV1Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
	);
	overrides_map.insert(
		EthereumStorageSchema::V2,
		Box::new(SchemaV2Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
	);
	overrides_map.insert(
		EthereumStorageSchema::V3,
		Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
	);

	Arc::new(OverrideHandle {
		schemas: overrides_map,
		fallback: Box::new(RuntimeApiStorageOverride::<B, C>::new(client)),
	})
}

pub fn onchain_storage_schema<B: BlockT, C, BE>(client: &C, hash: B::Hash) -> EthereumStorageSchema
where
	B: BlockT,
	C: HeaderBackend<B> + StorageProvider<B, BE>,
	BE: Backend<B>,
{
	match client.storage(hash, &StorageKey(PALLET_ETHEREUM_SCHEMA.to_vec())) {
		Ok(Some(bytes)) => Decode::decode(&mut &bytes.0[..])
			.ok()
			.unwrap_or(EthereumStorageSchema::Undefined),
		_ => EthereumStorageSchema::Undefined,
	}
}
