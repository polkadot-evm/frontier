// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! RPC types

mod account_info;
mod block;
mod block_count;
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

use ethereum::TransactionV3 as EthereumTransaction;
use ethereum_types::H160;

#[cfg(feature = "txpool")]
pub use self::txpool::{Summary, TransactionMap, TxPoolResult};
pub use self::{
	account_info::{AccountInfo, EthAccount, ExtAccountInfo, RecoveredAccount, StorageProof},
	block::{Block, BlockTransactions, Header, Rich, RichBlock, RichHeader},
	block_count::BlockCount,
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
