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
	use std::{collections::HashMap, path::PathBuf, sync::Arc};

	use fp_storage::EthereumStorageSchema;
	use sp_runtime::{
		generic::{Block, Header},
		traits::{BlakeTwo256, Block as BlockT},
	};
	use tempfile::tempdir;

	use crate::frontier_db_cmd::{Column, FrontierDbCmd, Operation};
	use ethereum_types::H256;
	use serde::Serialize;

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

	#[derive(Debug, Serialize)]
	#[serde(untagged)]
	enum TestValue {
		Schema(HashMap<H256, EthereumStorageSchema>),
		Tips(Vec<<OpaqueBlock as BlockT>::Hash>),
	}

	fn cmd(
		key: String,
		value: Option<PathBuf>,
		operation: Operation,
		column: Column,
	) -> FrontierDbCmd {
		FrontierDbCmd {
			operation,
			column,
			key,
			value,
			shared_params: sc_cli::SharedParams {
				chain: None,
				dev: true,
				base_path: None,
				log: vec![],
				disable_log_color: true,
				enable_log_reloading: true,
				tracing_targets: None,
				tracing_receiver: sc_cli::arg_enums::TracingReceiver::Log,
				detailed_log_output: false,
			},
		}
	}

	fn schema_test_value() -> TestValue {
		let mut inner = HashMap::new();
		inner.insert(H256::default(), EthereumStorageSchema::V1);
		TestValue::Schema(inner)
	}

	fn tips_test_value() -> TestValue {
		TestValue::Tips(vec![H256::default()])
	}

	fn test_json_file(tmp: &tempfile::TempDir, value: &TestValue) -> PathBuf {
		let test_value_path = tmp.path().join("test.json");
		std::fs::write(
			test_value_path.clone(),
			serde_json::to_string_pretty(value).unwrap(),
		)
		.expect("write test value json file");
		test_value_path
	}

	#[test]
	fn schema_create_success_if_value_is_empty() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = test_json_file(&tmp, &schema_test_value());

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		assert_eq!(backend.meta().ethereum_schema(), Ok(None));

		// Run the command
		assert!(cmd(
			":ethereum_schema_cache".to_string(),
			Some(test_value_path),
			Operation::Create,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());

		assert_eq!(
			backend.meta().ethereum_schema(),
			Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
		);
	}

	#[test]
	fn schema_create_fails_if_value_is_not_empty() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = test_json_file(&tmp, &schema_test_value());

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		let data_before = vec![(EthereumStorageSchema::V2, H256::default())];

		let _ = backend
			.meta()
			.write_ethereum_schema(data_before.clone())
			.expect("data inserted in temporary db");

		// Run the command
		assert!(cmd(
			":ethereum_schema_cache".to_string(),
			Some(test_value_path),
			Operation::Create,
			Column::Meta
		)
		.run(backend.clone())
		.is_err());

		let data_after = backend.meta().ethereum_schema().unwrap().unwrap();
		assert_eq!(data_after, data_before);
	}

	#[test]
	fn schema_read_works() {
		let tmp = tempdir().expect("create a temporary directory");

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		assert_eq!(backend.meta().ethereum_schema(), Ok(None));

		let data = vec![(EthereumStorageSchema::V2, H256::default())];

		let _ = backend
			.meta()
			.write_ethereum_schema(data.clone())
			.expect("data inserted in temporary db");

		// Run the command
		assert!(cmd(
			":ethereum_schema_cache".to_string(),
			None,
			Operation::Read,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());
	}

	#[test]
	fn schema_update_works() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = test_json_file(&tmp, &schema_test_value());

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		assert_eq!(backend.meta().ethereum_schema(), Ok(None));
		// Run the command
		assert!(cmd(
			":ethereum_schema_cache".to_string(),
			Some(test_value_path),
			Operation::Update,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());

		assert_eq!(
			backend.meta().ethereum_schema(),
			Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
		);
	}

	#[test]
	fn schema_delete_works() {
		let tmp = tempdir().expect("create a temporary directory");

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		let data = vec![(EthereumStorageSchema::V2, H256::default())];

		let _ = backend
			.meta()
			.write_ethereum_schema(data.clone())
			.expect("data inserted in temporary db");
		// Run the command
		assert!(cmd(
			":ethereum_schema_cache".to_string(),
			None,
			Operation::Delete,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());

		assert_eq!(backend.meta().ethereum_schema(), Ok(Some(vec![])));
	}

	#[test]
	fn tips_create_success_if_value_is_empty() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = test_json_file(&tmp, &tips_test_value());

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));
		// Run the command
		assert!(cmd(
			"CURRENT_SYNCING_TIPS".to_string(),
			Some(test_value_path),
			Operation::Create,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());

		assert_eq!(
			backend.meta().current_syncing_tips(),
			Ok(vec![H256::default()])
		);
	}

	#[test]
	fn tips_create_fails_if_value_is_not_empty() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = test_json_file(&tmp, &tips_test_value());

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		let data_before = vec![H256::default()];

		let _ = backend
			.meta()
			.write_current_syncing_tips(data_before.clone())
			.expect("data inserted in temporary db");
		// Run the command
		assert!(cmd(
			"CURRENT_SYNCING_TIPS".to_string(),
			Some(test_value_path),
			Operation::Create,
			Column::Meta
		)
		.run(backend.clone())
		.is_err());

		let data_after = backend.meta().current_syncing_tips().unwrap();
		assert_eq!(data_after, data_before);
	}

	#[test]
	fn tips_read_works() {
		let tmp = tempdir().expect("create a temporary directory");

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));

		let data = vec![H256::default()];

		let _ = backend
			.meta()
			.write_current_syncing_tips(data.clone())
			.expect("data inserted in temporary db");
		// Run the command
		assert!(cmd(
			"CURRENT_SYNCING_TIPS".to_string(),
			None,
			Operation::Read,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());
	}

	#[test]
	fn tips_update_works() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = test_json_file(&tmp, &tips_test_value());

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));
		// Run the command
		assert!(cmd(
			"CURRENT_SYNCING_TIPS".to_string(),
			Some(test_value_path),
			Operation::Update,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());

		assert_eq!(
			backend.meta().current_syncing_tips(),
			Ok(vec![H256::default()])
		);
	}

	#[test]
	fn tips_delete_works() {
		let tmp = tempdir().expect("create a temporary directory");

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		let data = vec![H256::default()];

		let _ = backend
			.meta()
			.write_current_syncing_tips(data.clone())
			.expect("data inserted in temporary db");
		// Run the command
		assert!(cmd(
			"CURRENT_SYNCING_TIPS".to_string(),
			None,
			Operation::Delete,
			Column::Meta
		)
		.run(backend.clone())
		.is_ok());

		assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));
	}

	#[test]
	fn non_existent_meta_static_keys_are_no_op() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = test_json_file(&tmp, &schema_test_value());

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		let data = vec![(EthereumStorageSchema::V1, H256::default())];

		let _ = backend
			.meta()
			.write_ethereum_schema(data.clone())
			.expect("data inserted in temporary db");

		// Run the Create command
		assert!(cmd(
			":foo".to_string(),
			Some(test_value_path.clone()),
			Operation::Create,
			Column::Meta
		)
		.run(backend.clone())
		.is_err());

		assert_eq!(
			backend.meta().ethereum_schema(),
			Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
		);

		// Run the Read command
		assert!(cmd(":foo".to_string(), None, Operation::Read, Column::Meta)
			.run(backend.clone())
			.is_err());

		// Run the Update command
		assert!(cmd(
			":foo".to_string(),
			Some(test_value_path),
			Operation::Update,
			Column::Meta
		)
		.run(backend.clone())
		.is_err());

		assert_eq!(
			backend.meta().ethereum_schema(),
			Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
		);

		// Run the Delete command
		assert!(
			cmd(":foo".to_string(), None, Operation::Delete, Column::Meta)
				.run(backend.clone())
				.is_err()
		);

		assert_eq!(
			backend.meta().ethereum_schema(),
			Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
		);
	}

	#[test]
	fn not_deserializable_input_value_is_no_op() {
		let tmp = tempdir().expect("create a temporary directory");
		// Write some data in a temp file.
		let test_value_path = tmp.path().join("test.json");

		std::fs::write(
			test_value_path.clone(),
			serde_json::to_string("im_not_allowed_here").unwrap(),
		)
		.expect("write test value json file");

		// Create a temporary frontier secondary DB.
		let backend = open_frontier_backend(tmp.into_path()).expect("a temporary db was created");

		// Run the Create command
		assert!(cmd(
			":ethereum_schema_cache".to_string(),
			Some(test_value_path.clone()),
			Operation::Create,
			Column::Meta
		)
		.run(backend.clone())
		.is_err());

		assert_eq!(backend.meta().ethereum_schema(), Ok(None));

		// Run the Update command
		assert!(cmd(
			":ethereum_schema_cache".to_string(),
			Some(test_value_path),
			Operation::Create,
			Column::Meta
		)
		.run(backend.clone())
		.is_err());

		assert_eq!(backend.meta().ethereum_schema(), Ok(None));
	}
}
