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

mod signature;
mod checked_extrinsic;
mod unchecked_extrinsic;

pub use crate::signature::MultiSignature;
pub use crate::checked_extrinsic::{CheckedSignature, CheckedExtrinsic};
pub use crate::unchecked_extrinsic::UncheckedExtrinsic;

use sp_core::{H256, H160};

pub trait EthereumOrigin {
	fn ethereum_transaction(sender: H160) -> Self;
}

pub trait EthereumAddress {
	fn ethereum_address(&self) -> Option<H160>;
}

pub trait EthereumTransaction {
	fn preimage_hash(&self) -> Option<H256>;
}
