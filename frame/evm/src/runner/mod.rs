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

pub mod stack;
pub mod builtin;

use sp_std::vec::Vec;
use sp_core::{H160, U256, H256};
use fp_evm::{CallInfo, CreateInfo};
use crate::Config;

pub trait Runner<T: Config> {
	type Error: Into<sp_runtime::DispatchError>;

	fn call(
		source: H160,
		target: H160,
		input: Vec<u8>,
		value: U256,
		gas_limit: u64,
		gas_price: Option<U256>,
		nonce: Option<U256>,
		config: &evm::Config,
	) -> Result<CallInfo, Self::Error>;

	fn create(
		source: H160,
		init: Vec<u8>,
		value: U256,
		gas_limit: u64,
		gas_price: Option<U256>,
		nonce: Option<U256>,
		config: &evm::Config,
	) -> Result<CreateInfo, Self::Error>;

	fn create2(
		source: H160,
		init: Vec<u8>,
		salt: H256,
		value: U256,
		gas_limit: u64,
		gas_price: Option<U256>,
		nonce: Option<U256>,
		config: &evm::Config,
	) -> Result<CreateInfo, Self::Error>;
}
