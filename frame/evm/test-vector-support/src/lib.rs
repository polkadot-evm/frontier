// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
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

use std::fs;

use evm::{Context, ExitSucceed};
use fp_evm::Precompile;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EthConsensusTest {
	input: String,
	expected: String,
	name: String,
	gas: Option<u64>,
}

/// Tests a precompile against the ethereum consensus tests defined in the given file at filepath.
/// The file is expected to be in JSON format and contain an array of test vectors, where each
/// vector can be deserialized into an "EthConsensusTest".
pub fn test_precompile_test_vectors<P: Precompile>(filepath: &str) -> Result<(), String> {
	let data = fs::read_to_string(&filepath).expect("Failed to read blake2F.json");

	let tests: Vec<EthConsensusTest> = serde_json::from_str(&data).expect("expected json array");

	for test in tests {
		let input: Vec<u8> = hex::decode(test.input).expect("Could not hex-decode test input data");

		let cost: u64 = 10000000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		match P::execute(&input, Some(cost), &context, false) {
			Ok(result) => {
				let as_hex: String = hex::encode(result.output);
				assert_eq!(
					result.exit_status,
					ExitSucceed::Returned,
					"test '{}' returned {:?} (expected 'Returned')",
					test.name,
					result.exit_status
				);
				assert_eq!(
					as_hex, test.expected,
					"test '{}' failed (different output)",
					test.name
				);
				if let Some(expected_gas) = test.gas {
					assert_eq!(
						result.cost, expected_gas,
						"test '{}' failed (different gas cost)",
						test.name
					);
				}
			}
			Err(err) => {
				return Err(format!("Test '{}' returned error: {:?}", test.name, err));
			}
		}
	}

	Ok(())
}
