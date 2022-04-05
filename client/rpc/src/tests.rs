#[cfg(test)]
mod tests {
	use crate::{frontier_backend_client, EthTask};

	use codec::Encode;
	use std::{sync::Arc, thread, time};

	use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};
	use frontier_template_runtime::RuntimeApi;
	use futures::executor;
	use sc_block_builder::BlockBuilderProvider;
	use sp_consensus::BlockOrigin;
	use sp_core::traits::SpawnEssentialNamed;
	use sp_runtime::{
		generic::{Block, Header},
		traits::BlakeTwo256,
	};
	use substrate_test_runtime_client::{
		prelude::*, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};

	type OpaqueBlock =
		Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

	pub const DB_NAME: &str = "testfrontierdb";

	pub fn open_frontier_backend() -> Result<Arc<fc_db::Backend<OpaqueBlock>>, String> {
		Ok(Arc::new(fc_db::Backend::<OpaqueBlock>::new(
			&fc_db::DatabaseSettings {
				source: fc_db::DatabaseSettingsSrc::RocksDb {
					path: std::env::temp_dir().join(DB_NAME),
					cache_size: 0,
				},
			},
		)?))
	}

	struct Env;
	impl Drop for Env {
		fn drop(&mut self) {
			let _ = std::fs::remove_dir_all(std::env::temp_dir().join(DB_NAME));
		}
	}

	#[test]
	fn should_cache_pallet_ethereum_schema() {
		// Setup cleansing.
		let _env = Env;

		// Initialize storage with schema V1.
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V1),
		);
		let (client, _) = builder.build_with_native_executor::<RuntimeApi, _>(None);
		let mut client = Arc::new(client);

		// Create a temporary frontier secondary DB.
		let frontier_backend = open_frontier_backend().unwrap();

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
		std::thread::sleep(time::Duration::from_millis(10));

		// Expect: genesis still cached (V1), latest block cached (V2)
		assert_eq!(
			frontier_backend_client::load_cached_schema::<_>(frontier_backend.as_ref()),
			Ok(Some(vec![
				(EthereumStorageSchema::V1, client.genesis_hash()),
				(EthereumStorageSchema::V2, block_hash)
			]))
		);
	}
}
