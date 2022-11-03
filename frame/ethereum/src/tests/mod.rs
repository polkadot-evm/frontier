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
