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

use super::*;
use pallet_evm_test_vector_support::{
	test_precompile_failure_test_vectors, test_precompile_test_vectors,
};

#[test]
fn process_consensus_tests() -> Result<(), String> {
	test_precompile_test_vectors::<Bls12381G1Add>("../testdata/bls12381G1Add.json")?;
	test_precompile_test_vectors::<Bls12381G1Mul>("../testdata/bls12381G1Mul.json")?;
	test_precompile_test_vectors::<Bls12381G1MultiExp>("../testdata/bls12381G1MultiExp.json")?;
	test_precompile_test_vectors::<Bls12381G2Add>("../testdata/bls12381G2Add.json")?;
	test_precompile_test_vectors::<Bls12381G2Mul>("../testdata/bls12381G2Mul.json")?;
	test_precompile_test_vectors::<Bls12381G2MultiExp>("../testdata/bls12381G2MultiExp.json")?;
	test_precompile_test_vectors::<Bls12381Pairing>("../testdata/bls12381Pairing.json")?;
	test_precompile_test_vectors::<Bls12381MapG1>("../testdata/bls12381MapG1.json")?;
	test_precompile_test_vectors::<Bls12381MapG2>("../testdata/bls12381MapG2.json")?;
	Ok(())
}

#[test]
fn process_consensus_failure_tests() -> Result<(), String> {
	test_precompile_failure_test_vectors::<Bls12381G1Add>("../testdata/fail-bls12381G1Add.json")?;
	test_precompile_failure_test_vectors::<Bls12381G1Mul>("../testdata/fail-bls12381G1Mul.json")?;
	test_precompile_failure_test_vectors::<Bls12381G1MultiExp>(
		"../testdata/fail-bls12381G1MultiExp.json",
	)?;
	test_precompile_failure_test_vectors::<Bls12381G2Add>("../testdata/fail-bls12381G2Add.json")?;
	test_precompile_failure_test_vectors::<Bls12381G2Mul>("../testdata/fail-bls12381G2Mul.json")?;
	test_precompile_failure_test_vectors::<Bls12381G2MultiExp>(
		"../testdata/fail-bls12381G2MultiExp.json",
	)?;
	test_precompile_failure_test_vectors::<Bls12381Pairing>(
		"../testdata/fail-bls12381Pairing.json",
	)?;
	test_precompile_failure_test_vectors::<Bls12381MapG1>("../testdata/fail-bls12381MapG1.json")?;
	test_precompile_failure_test_vectors::<Bls12381MapG2>("../testdata/fail-bls12381MapG2.json")?;
	Ok(())
}
