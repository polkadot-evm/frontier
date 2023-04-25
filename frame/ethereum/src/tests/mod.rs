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

use frame_support::{
	assert_err, assert_ok, dispatch::GetDispatchInfo, unsigned::TransactionValidityError,
};
use sp_runtime::{
	traits::Applyable,
	transaction_validity::{InvalidTransaction, ValidTransactionBuilder},
};
use std::str::FromStr;

use crate::{
	mock::*, CallOrCreateInfo, RawOrigin, Transaction, TransactionAction, H160, H256, U256,
};
use fp_self_contained::CheckedExtrinsic;
use fp_evm::{ACCOUNT_BASIC_PROOF_SIZE, ACCOUNT_CODES_METADATA_PROOF_SIZE, ACCOUNT_STORAGE_PROOF_SIZE,
HASH_PROOF_SIZE,};

mod eip1559;
mod eip2930;
mod legacy;

// This ERC-20 contract mints the maximum amount of tokens to the contract creator.
// pragma solidity ^0.5.0;`
// import "https://github.com/OpenZeppelin/openzeppelin-contracts/blob/v2.5.1/contracts/token/ERC20/ERC20.sol";
// contract MyToken is ERC20 {
//	 constructor() public { _mint(msg.sender, 2**256 - 1); }
// }
pub const ERC20_CONTRACT_BYTECODE: &str = include_str!("./res/erc20_contract_bytecode.txt");

// pragma solidity ^0.8.2;
// contract Callee {
//     // ac4c25b2
//     function void() public {
//         uint256 foo = 1;
//     }
// }
pub const PROOF_SIZE_TEST_CALLEE_CONTRACT_BYTECODE: &str = include_str!("./res/proof_size_test_callee_contract_bytecode.txt");
// pragma solidity ^0.8.2;
// contract ProofSizeTest {

//     uint256 foo;

//     constructor() {
//         foo = 6;
//     }

//     // 35f56c3b
//     function test_balance(address who) public {
//         // cold
//         uint256 a = address(who).balance;
//         // warm
//         uint256 b = address(who).balance;
//     }
//     // e27a0ecd
//     function test_sload() public returns (uint256) {
//         // cold
//         uint256 a = foo;
//         // warm
//         uint256 b = foo;
//         return b;
//     }
//     // 4f3080a9
//     function test_sstore() public {
//         // cold
//         foo = 4;
//         // warm
//         foo = 5;
//     }
//     // c6d6f606
//     function test_call(Callee _callee) public {
//         _callee.void();
//     }
//     // 944ddc62
//     function test_oog() public {
//         uint256 i = 1;
//         while(true) {
//             address who = address(uint160(uint256(keccak256(abi.encodePacked(bytes32(i))))));
//             uint256 a = address(who).balance;
//             i = i + 1;
//         }
//     }
// }
pub const PROOF_SIZE_TEST_CONTRACT_BYTECODE: &str = include_str!("./res/proof_size_test_contract_bytecode.txt");
