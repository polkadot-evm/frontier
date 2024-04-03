// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

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

//! RPC types

mod account_info;
mod block;
mod block_number;
mod bytes;
mod call_request;
mod fee;
mod filter;
mod index;
mod log;
mod receipt;
mod sync;
mod transaction;
mod transaction_request;
#[cfg(feature = "txpool")]
mod txpool;
mod work;

pub mod pubsub;

use ethereum::TransactionV2 as EthereumTransaction;
use ethereum_types::H160;

#[cfg(feature = "txpool")]
pub use self::txpool::{Summary, TransactionMap, TxPoolResult};
pub use self::{
	account_info::{AccountInfo, EthAccount, ExtAccountInfo, RecoveredAccount, StorageProof},
	block::{Block, BlockTransactions, Header, Rich, RichBlock, RichHeader},
	block_number::BlockNumberOrHash,
	bytes::Bytes,
	call_request::CallStateOverride,
	fee::{FeeHistory, FeeHistoryCache, FeeHistoryCacheItem, FeeHistoryCacheLimit},
	filter::{
		Filter, FilterAddress, FilterChanges, FilterPool, FilterPoolItem, FilterType,
		FilteredParams, Topic, VariadicValue,
	},
	index::Index,
	log::Log,
	receipt::Receipt,
	sync::{
		ChainStatus, EthProtocolInfo, PeerCount, PeerInfo, PeerNetworkInfo, PeerProtocolsInfo,
		Peers, PipProtocolInfo, SyncInfo, SyncStatus, TransactionStats,
	},
	transaction::{LocalTransactionStatus, RichRawTransaction, Transaction},
	transaction_request::{TransactionMessage, TransactionRequest},
	work::Work,
};

/// The trait that used to build types from the `from` address and ethereum `transaction`.
pub trait BuildFrom {
	fn build_from(from: H160, transaction: &EthereumTransaction) -> Self;
}
