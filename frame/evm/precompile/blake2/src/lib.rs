// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod eip_152;

use alloc::vec::Vec;
use core::mem::size_of;
use fp_evm::Precompile;
use evm::{ExitSucceed, ExitError, Context};

pub struct Blake2F;

impl Precompile for Blake2F {

	/// Format of `input`:
	/// [4 bytes for rounds][64 bytes for h][128 bytes for m][8 bytes for t_0][8 bytes for t_1][1 byte for f]
	fn execute(
		input: &[u8],
		_target_gas: Option<u64>,
		_context: &Context,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, u64), ExitError> {
		const BLAKE2_F_ARG_LEN: usize = 213;

		if input.len() != BLAKE2_F_ARG_LEN {
			return Err(ExitError::Other("input length for Blake2 F precompile should be exactly 213 bytes".into()));
		}

		let mut rounds_buf: [u8; 4] = [0; 4];
		rounds_buf.copy_from_slice(&input[0..4]);
		let rounds: u32 = u32::from_be_bytes(rounds_buf);

		let mut h_buf: [u8; 64] = [0; 64];
		h_buf.copy_from_slice(&input[4..68]);
		let mut h = [0u64; 8];
		let mut ctr = 0;
		for state_word in &mut h {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&h_buf[(ctr * 8)..(ctr + 1) * 8]);
			*state_word = u64::from_ne_bytes(temp).into();
			ctr += 1;
		}

		let mut m_buf: [u8; 128] = [0; 128];
		m_buf.copy_from_slice(&input[68..196]);
		let mut m = [0u64; 16];
		ctr = 0;
		for msg_word in &mut m {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&m_buf[(ctr * 8)..(ctr + 1) * 8]);
			*msg_word = u64::from_ne_bytes(temp).into();
			ctr += 1;
		}


		let mut t_0_buf: [u8; 8] = [0; 8];
		t_0_buf.copy_from_slice(&input[196..204]);
		let t_0 = u64::from_ne_bytes(t_0_buf);

		let mut t_1_buf: [u8; 8] = [0; 8];
		t_1_buf.copy_from_slice(&input[204..212]);
		let t_1 = u64::from_ne_bytes(t_1_buf);

		let f = if input[212] == 1 { true } else if input[212] == 0 { false } else {
			return Err(ExitError::Other("incorrect final block indicator flag".into()))
		};

		crate::eip_152::compress(&mut h, m, [t_0.into(), t_1.into()], f, rounds as usize);

		let mut output_buf = [0u8; 8 * size_of::<u64>()];
		for (i, state_word) in h.iter().enumerate() {
			output_buf[i * 8..(i + 1) * 8].copy_from_slice(&state_word.to_le_bytes());
		}

		Ok((ExitSucceed::Returned, output_buf.to_vec(), 0))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	extern crate hex;
	use std::fs;
	use serde::Deserialize;

	#[allow(non_snake_case)]
	#[derive(Deserialize, Debug)]
	struct EthConsensusTest {
		Input: String,
		Expected: String,
		Name: String,
		Gas: u64,
	}

	/// Tests a precompile against the ethereum consensus tests defined in the given file at filepath.
	/// The file is expected to be in JSON format and contain an array of test vectors, where each
	/// vector can be deserialized into an "EthConsensusTest".
	pub fn test_precompile_consensus_tests<P: Precompile>(filepath: &str)
		-> std::result::Result<(), String>
	{
		let data = fs::read_to_string(&filepath)
			.expect("Failed to read blake2F.json");

		let tests: Vec<EthConsensusTest> = serde_json::from_str(&data)
			.expect("expected json array");

		for test in tests {
			let input: Vec<u8> = hex::decode(test.Input)
				.expect("Could not hex-decode test input data");

			let cost: u64 = 1000000;

			let context: Context = Context {
				address: Default::default(),
				caller: Default::default(),
				apparent_value: From::from(0),
			};

			match P::execute(&input, Some(cost), &context) {
				Ok((_, output, gas)) => {
					let as_hex: String = hex::encode(output);
					assert_eq!(as_hex, test.Expected,
							   "test '{}' failed (different output)", test.Name);
					assert_eq!(gas, test.Gas,
							   "test '{}' failed (different gas cost)", test.Name);
				},
				Err(_) => {
					panic!("Precompile::execute() returned error");
				}
			}
		}

		Ok(())
	}



	// TODO: DRY
	#[test]
	fn process_consensus_tests() -> std::result::Result<(), String> {
		test_precompile_consensus_tests::<Blake2F>("./blake2F.json")?;
		Ok(())
	}
}
