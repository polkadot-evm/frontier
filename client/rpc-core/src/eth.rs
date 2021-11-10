// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2020 Parity Technologies (UK) Ltd.
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

//! Eth rpc interface.

use ethereum_types::{H160, H256, H64, U256, U64};
use jsonrpc_core::{BoxFuture, Result};
use jsonrpc_derive::rpc;

use crate::types::{
	BlockNumber, Bytes, CallRequest, FeeHistory, Filter, FilterChanges, Index, Log, Receipt,
	RichBlock, SyncStatus, Transaction, TransactionRequest, Work,
};
pub use rpc_impl_EthApi::gen_server::EthApi as EthApiServer;
pub use rpc_impl_EthFilterApi::gen_server::EthFilterApi as EthFilterApiServer;

/// Eth rpc interface.
#[rpc(server)]
pub trait EthApi {
	/// Returns protocol version encoded as a string (quotes are necessary).
	#[rpc(name = "eth_protocolVersion")]
	fn protocol_version(&self) -> Result<u64>;

	/// Returns an object with data about the sync status or false. (wtf?)
	#[rpc(name = "eth_syncing")]
	fn syncing(&self) -> Result<SyncStatus>;

	/// Returns the number of hashes per second that the node is mining with.
	#[rpc(name = "eth_hashrate")]
	fn hashrate(&self) -> Result<U256>;

	/// Returns block author.
	#[rpc(name = "eth_coinbase")]
	fn author(&self) -> Result<H160>;

	/// Returns true if client is actively mining new blocks.
	#[rpc(name = "eth_mining")]
	fn is_mining(&self) -> Result<bool>;

	/// Returns the chain ID used for transaction signing at the
	/// current best block. None is returned if not
	/// available.
	#[rpc(name = "eth_chainId")]
	fn chain_id(&self) -> Result<Option<U64>>;

	/// Returns current gas_price.
	#[rpc(name = "eth_gasPrice")]
	fn gas_price(&self) -> Result<U256>;

	/// Returns accounts list.
	#[rpc(name = "eth_accounts")]
	fn accounts(&self) -> Result<Vec<H160>>;

	/// Returns highest block number.
	#[rpc(name = "eth_blockNumber")]
	fn block_number(&self) -> Result<U256>;

	/// Returns balance of the given account.
	#[rpc(name = "eth_getBalance")]
	fn balance(&self, _: H160, _: Option<BlockNumber>) -> Result<U256>;

	/// Returns content of the storage at given address.
	#[rpc(name = "eth_getStorageAt")]
	fn storage_at(&self, _: H160, _: U256, _: Option<BlockNumber>) -> Result<H256>;

	/// Returns block with given hash.
	#[rpc(name = "eth_getBlockByHash")]
	fn block_by_hash(&self, _: H256, _: bool) -> Result<Option<RichBlock>>;

	/// Returns block with given number.
	#[rpc(name = "eth_getBlockByNumber")]
	fn block_by_number(&self, _: BlockNumber, _: bool) -> Result<Option<RichBlock>>;

	/// Returns the number of transactions sent from given address at given time (block number).
	#[rpc(name = "eth_getTransactionCount")]
	fn transaction_count(&self, _: H160, _: Option<BlockNumber>) -> Result<U256>;

	/// Returns the number of transactions in a block with given hash.
	#[rpc(name = "eth_getBlockTransactionCountByHash")]
	fn block_transaction_count_by_hash(&self, _: H256) -> Result<Option<U256>>;

	/// Returns the number of transactions in a block with given block number.
	#[rpc(name = "eth_getBlockTransactionCountByNumber")]
	fn block_transaction_count_by_number(&self, _: BlockNumber) -> Result<Option<U256>>;

	/// Returns the number of uncles in a block with given hash.
	#[rpc(name = "eth_getUncleCountByBlockHash")]
	fn block_uncles_count_by_hash(&self, _: H256) -> Result<U256>;

	/// Returns the number of uncles in a block with given block number.
	#[rpc(name = "eth_getUncleCountByBlockNumber")]
	fn block_uncles_count_by_number(&self, _: BlockNumber) -> Result<U256>;

	/// Returns the code at given address at given time (block number).
	#[rpc(name = "eth_getCode")]
	fn code_at(&self, _: H160, _: Option<BlockNumber>) -> Result<Bytes>;

	/// Sends transaction; will block waiting for signer to return the
	/// transaction hash.
	#[rpc(name = "eth_sendTransaction")]
	fn send_transaction(&self, _: TransactionRequest) -> BoxFuture<Result<H256>>;

	/// Sends signed transaction, returning its hash.
	#[rpc(name = "eth_sendRawTransaction")]
	fn send_raw_transaction(&self, _: Bytes) -> BoxFuture<Result<H256>>;

	/// Call contract, returning the output data.
	#[rpc(name = "eth_call")]
	fn call(&self, _: CallRequest, _: Option<BlockNumber>) -> Result<Bytes>;

	/// Estimate gas needed for execution of given contract.
	#[rpc(name = "eth_estimateGas")]
	fn estimate_gas(&self, _: CallRequest, _: Option<BlockNumber>) -> Result<U256>;

	/// Get transaction by its hash.
	#[rpc(name = "eth_getTransactionByHash")]
	fn transaction_by_hash(&self, _: H256) -> Result<Option<Transaction>>;

	/// Returns transaction at given block hash and index.
	#[rpc(name = "eth_getTransactionByBlockHashAndIndex")]
	fn transaction_by_block_hash_and_index(&self, _: H256, _: Index)
		-> Result<Option<Transaction>>;

	/// Returns transaction by given block number and index.
	#[rpc(name = "eth_getTransactionByBlockNumberAndIndex")]
	fn transaction_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> Result<Option<Transaction>>;

	/// Returns transaction receipt by transaction hash.
	#[rpc(name = "eth_getTransactionReceipt")]
	fn transaction_receipt(&self, _: H256) -> Result<Option<Receipt>>;

	/// Returns an uncles at given block and index.
	#[rpc(name = "eth_getUncleByBlockHashAndIndex")]
	fn uncle_by_block_hash_and_index(&self, _: H256, _: Index) -> Result<Option<RichBlock>>;

	/// Returns an uncles at given block and index.
	#[rpc(name = "eth_getUncleByBlockNumberAndIndex")]
	fn uncle_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> Result<Option<RichBlock>>;

	/// Returns logs matching given filter object.
	#[rpc(name = "eth_getLogs")]
	fn logs(&self, _: Filter) -> Result<Vec<Log>>;

	/// Returns the hash of the current block, the seedHash, and the boundary condition to be met.
	#[rpc(name = "eth_getWork")]
	fn work(&self) -> Result<Work>;

	/// Used for submitting a proof-of-work solution.
	#[rpc(name = "eth_submitWork")]
	fn submit_work(&self, _: H64, _: H256, _: H256) -> Result<bool>;

	/// Used for submitting mining hashrate.
	#[rpc(name = "eth_submitHashrate")]
	fn submit_hashrate(&self, _: U256, _: H256) -> Result<bool>;

	/// Introduced in EIP-1159 for getting information on the appropiate priority fee to use.
	#[rpc(name = "eth_feeHistory")]
	fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistory>;
}

/// Eth filters rpc api (polling).
#[rpc(server)]
pub trait EthFilterApi {
	/// Returns id of new filter.
	#[rpc(name = "eth_newFilter")]
	fn new_filter(&self, _: Filter) -> Result<U256>;

	/// Returns id of new block filter.
	#[rpc(name = "eth_newBlockFilter")]
	fn new_block_filter(&self) -> Result<U256>;

	/// Returns id of new block filter.
	#[rpc(name = "eth_newPendingTransactionFilter")]
	fn new_pending_transaction_filter(&self) -> Result<U256>;

	/// Returns filter changes since last poll.
	#[rpc(name = "eth_getFilterChanges")]
	fn filter_changes(&self, _: Index) -> Result<FilterChanges>;

	/// Returns all logs matching given filter (in a range 'from' - 'to').
	#[rpc(name = "eth_getFilterLogs")]
	fn filter_logs(&self, _: Index) -> Result<Vec<Log>>;

	/// Uninstalls filter.
	#[rpc(name = "eth_uninstallFilter")]
	fn uninstall_filter(&self, _: Index) -> Result<bool>;
}
