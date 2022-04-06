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

#[cfg(test)]
mod tests {
	use crate::{frontier_backend_client, EthTask};

	use codec::Encode;
	use std::{path::PathBuf, sync::Arc, thread, time};

	use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};
	use frontier_template_runtime::RuntimeApi;
	use futures::executor;
	use sc_block_builder::BlockBuilderProvider;
	use sp_consensus::BlockOrigin;
	use sp_core::traits::SpawnEssentialNamed;
	use sp_runtime::{
		generic::{Block, BlockId, Header},
		traits::BlakeTwo256,
	};
	use substrate_test_runtime_client::{
		prelude::*, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	use tempfile::tempdir;

	type OpaqueBlock =
		Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

	pub fn open_frontier_backend(
		path: PathBuf,
	) -> Result<Arc<fc_db::Backend<OpaqueBlock>>, String> {
		Ok(Arc::new(fc_db::Backend::<OpaqueBlock>::new(
			&fc_db::DatabaseSettings {
				source: fc_db::DatabaseSettingsSrc::RocksDb {
					path,
					cache_size: 0,
				},
			},
		)?))
	}

	#[test]
	fn should_cache_pallet_ethereum_schema() {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V1.
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V1),
		);
		let (client, _) = builder.build_with_native_executor::<RuntimeApi, _>(None);
		let mut client = Arc::new(client);

		// Create a temporary frontier secondary DB.
		let frontier_backend = open_frontier_backend(tmp.into_path()).unwrap();

		// Spawn `frontier-schema-cache-task` background task.
		let spawner = sp_core::testing::TaskExecutor::new();
		spawner.spawn_essential_blocking(
			"frontier-schema-cache-task",
			None,
			Box::pin(EthTask::ethereum_schema_cache_task(
				Arc::clone(&client),
				Arc::clone(&frontier_backend),
			)),
		);

		// Create some blocks.
		for nonce in [1, 2, 3, 4, 5].into_iter() {
			let mut builder = client.new_block(Default::default()).unwrap();
			builder.push_storage_change(vec![nonce], None).unwrap();
			let block = builder.build().unwrap().block;
			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
		}

		// Expect: only genesis block is cached to schema V1.
		assert_eq!(
			frontier_backend_client::load_cached_schema::<_>(frontier_backend.as_ref()),
			Ok(Some(vec![(
				EthereumStorageSchema::V1,
				client.genesis_hash()
			)]))
		);

		// Create another block and push a schema change (V2).
		let mut builder = client.new_block(Default::default()).unwrap();
		builder
			.push_storage_change(
				PALLET_ETHEREUM_SCHEMA.to_vec(),
				Some(Encode::encode(&EthereumStorageSchema::V2)),
			)
			.unwrap();
		let block = builder.build().unwrap().block;
		let block_hash = block.header.hash();
		executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

		// Give some time to consume and process the import notification stream.
		thread::sleep(time::Duration::from_millis(1));

		// Expect: genesis still cached (V1), latest block cached (V2)
		assert_eq!(
			frontier_backend_client::load_cached_schema::<_>(frontier_backend.as_ref()),
			Ok(Some(vec![
				(EthereumStorageSchema::V1, client.genesis_hash()),
				(EthereumStorageSchema::V2, block_hash)
			]))
		);
	}

	#[test]
	fn should_handle_cache_on_multiple_forks() {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V1.
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V1),
		);
		let (client, _) = builder.build_with_native_executor::<RuntimeApi, _>(None);
		let mut client = Arc::new(client);

		// Create a temporary frontier secondary DB.
		let frontier_backend = open_frontier_backend(tmp.into_path()).unwrap();

		// Spawn `frontier-schema-cache-task` background task.
		let spawner = sp_core::testing::TaskExecutor::new();
		spawner.spawn_essential_blocking(
			"frontier-schema-cache-task",
			None,
			Box::pin(EthTask::ethereum_schema_cache_task(
				Arc::clone(&client),
				Arc::clone(&frontier_backend),
			)),
		);

		// G -> A1.
		let mut builder = client.new_block(Default::default()).unwrap();
		builder.push_storage_change(vec![1], None).unwrap();
		let a1 = builder.build().unwrap().block;
		let a1_hash = a1.header.hash();
		executor::block_on(client.import(BlockOrigin::Own, a1)).unwrap();

		// A1 -> A2, we store V2 schema.
		let mut builder = client
			.new_block_at(&BlockId::Hash(a1_hash), Default::default(), false)
			.unwrap();
		builder
			.push_storage_change(
				PALLET_ETHEREUM_SCHEMA.to_vec(),
				Some(Encode::encode(&EthereumStorageSchema::V2)),
			)
			.unwrap();
		let a2 = builder.build().unwrap().block;
		let a2_hash = a2.header.hash();
		executor::block_on(client.import(BlockOrigin::Own, a2)).unwrap();

		// Give some time to consume and process the import notification stream.
		thread::sleep(time::Duration::from_millis(1));

		// Expect: genesis with schema V1, A2 with schema V2.
		assert_eq!(
			frontier_backend_client::load_cached_schema::<_>(frontier_backend.as_ref()),
			Ok(Some(vec![
				(EthereumStorageSchema::V1, client.genesis_hash()),
				(EthereumStorageSchema::V2, a2_hash)
			]))
		);

		// A1 -> B2. A new block on top of A1.
		let mut builder = client
			.new_block_at(&BlockId::Hash(a1_hash), Default::default(), false)
			.unwrap();
		builder.push_storage_change(vec![2], None).unwrap();
		let b2 = builder.build().unwrap().block;
		let b2_hash = b2.header.hash();
		executor::block_on(client.import(BlockOrigin::Own, b2)).unwrap();

		// B2 -> B3, we store V2 schema again. This is the longest chain.
		let mut builder = client
			.new_block_at(&BlockId::Hash(b2_hash), Default::default(), false)
			.unwrap();
		builder
			.push_storage_change(
				PALLET_ETHEREUM_SCHEMA.to_vec(),
				Some(Encode::encode(&EthereumStorageSchema::V2)),
			)
			.unwrap();
		let b3 = builder.build().unwrap().block;
		let b3_hash = b3.header.hash();
		executor::block_on(client.import(BlockOrigin::Own, b3)).unwrap();

		// Give some time to consume and process the import notification stream.
		thread::sleep(time::Duration::from_millis(1));

		// Expect: A2 to be retracted, genesis with schema V1, B3 with schema V2.
		assert_eq!(
			frontier_backend_client::load_cached_schema::<_>(frontier_backend.as_ref()),
			Ok(Some(vec![
				(EthereumStorageSchema::V1, client.genesis_hash()),
				(EthereumStorageSchema::V2, b3_hash)
			]))
		);

		// A1 -> C2, a wild new block on top of A1.
		let mut builder = client
			.new_block_at(&BlockId::Hash(a1_hash), Default::default(), false)
			.unwrap();
		builder
			.push_storage_change(
				PALLET_ETHEREUM_SCHEMA.to_vec(),
				Some(Encode::encode(&EthereumStorageSchema::V2)),
			)
			.unwrap();
		builder.push_storage_change(vec![3], None).unwrap();
		let c2 = builder.build().unwrap().block;
		let c2_hash = c2.header.hash();
		executor::block_on(client.import(BlockOrigin::Own, c2)).unwrap();

		// Give some time to consume and process the import notification stream.
		thread::sleep(time::Duration::from_millis(1));

		// Expect: genesis with schema V1, B3 still with schema V2.
		// C2 still not best block and not cached.
		assert_eq!(
			frontier_backend_client::load_cached_schema::<_>(frontier_backend.as_ref()),
			Ok(Some(vec![
				(EthereumStorageSchema::V1, client.genesis_hash()),
				(EthereumStorageSchema::V2, b3_hash)
			]))
		);

		// Make C2 branch the longest chain.
		// C2 -> D2
		let mut builder = client
			.new_block_at(&BlockId::Hash(c2_hash), Default::default(), false)
			.unwrap();
		builder.push_storage_change(vec![2], None).unwrap();
		let d2 = builder.build().unwrap().block;
		let d2_hash = d2.header.hash();
		executor::block_on(client.import(BlockOrigin::Own, d2)).unwrap();

		// D2 -> E2
		let mut builder = client
			.new_block_at(&BlockId::Hash(d2_hash), Default::default(), false)
			.unwrap();
		builder.push_storage_change(vec![3], None).unwrap();
		let e2 = builder.build().unwrap().block;
		executor::block_on(client.import(BlockOrigin::Own, e2)).unwrap();

		// Give some time to consume and process the import notification stream.
		thread::sleep(time::Duration::from_millis(1));

		// Expect: B2 branch to be retracted, genesis with schema V1, C2 with schema V2.
		// E2 became new best, chain reorged, we expect the C2 ancestor to be cached.
		assert_eq!(
			frontier_backend_client::load_cached_schema::<_>(frontier_backend.as_ref()),
			Ok(Some(vec![
				(EthereumStorageSchema::V1, client.genesis_hash()),
				(EthereumStorageSchema::V2, c2_hash)
			]))
		);
	}
}
