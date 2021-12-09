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
use jsonrpsee::{proc_macros::rpc, types::RpcResult};

use crate::types::{
	BlockNumber, Bytes, CallRequest, Filter, FilterChanges, Index, Log, Receipt, RichBlock,
	SyncStatus, Transaction, TransactionRequest, Work,
};

/// Eth rpc interface.
#[rpc(server)]
pub trait EthApi {
	/// Returns protocol version encoded as a string (quotes are necessary).
	#[method(name = "eth_protocolVersion")]
	fn protocol_version(&self) -> RpcResult<u64>;

	/// Returns an object with data about the sync status or false. (wtf?)
	#[method(name = "eth_syncing")]
	fn syncing(&self) -> RpcResult<SyncStatus>;

	/// Returns the number of hashes per second that the node is mining with.
	#[method(name = "eth_hashrate")]
	fn hashrate(&self) -> RpcResult<U256>;

	/// Returns block author.
	#[method(name = "eth_coinbase")]
	fn author(&self) -> RpcResult<H160>;

	/// Returns true if client is actively mining new blocks.
	#[method(name = "eth_mining")]
	fn is_mining(&self) -> RpcResult<bool>;

	/// Returns the chain ID used for transaction signing at the
	/// current best block. None is returned if not
	/// available.
	#[method(name = "eth_chainId")]
	fn chain_id(&self) -> RpcResult<Option<U64>>;

	/// Returns current gas_price.
	#[method(name = "eth_gasPrice")]
	fn gas_price(&self) -> RpcResult<U256>;

	/// Returns accounts list.
	#[method(name = "eth_accounts")]
	fn accounts(&self) -> RpcResult<Vec<H160>>;

	/// Returns highest block number.
	#[method(name = "eth_blockNumber")]
	fn block_number(&self) -> RpcResult<U256>;

	/// Returns balance of the given account.
	#[method(name = "eth_getBalance")]
	fn balance(&self, address: H160, block_number: Option<BlockNumber>) -> RpcResult<U256>;

	/// Returns content of the storage at given address.
	#[method(name = "eth_getStorageAt")]
	fn storage_at(
		&self,
		address: H160,
		key: U256,
		block_number: Option<BlockNumber>,
	) -> RpcResult<H256>;

	/// Returns block with given hash.
	#[method(name = "eth_getBlockByHash")]
	fn block_by_hash(&self, hash: H256, b: bool) -> RpcResult<Option<RichBlock>>;

	/// Returns block with given number.
	#[method(name = "eth_getBlockByNumber")]
	fn block_by_number(&self, block_number: BlockNumber, b: bool) -> RpcResult<Option<RichBlock>>;

	/// Returns the number of transactions sent from given address at given time (block number).
	#[method(name = "eth_getTransactionCount")]
	fn transaction_count(
		&self,
		address: H160,
		block_number: Option<BlockNumber>,
	) -> RpcResult<U256>;

	/// Returns the number of transactions in a block with given hash.
	#[method(name = "eth_getBlockTransactionCountByHash")]
	fn block_transaction_count_by_hash(&self, hash: H256) -> RpcResult<Option<U256>>;

	/// Returns the number of transactions in a block with given block number.
	#[method(name = "eth_getBlockTransactionCountByNumber")]
	fn block_transaction_count_by_number(
		&self,
		block_number: BlockNumber,
	) -> RpcResult<Option<U256>>;

	/// Returns the number of uncles in a block with given hash.
	#[method(name = "eth_getUncleCountByBlockHash")]
	fn block_uncles_count_by_hash(&self, hash: H256) -> RpcResult<U256>;

	/// Returns the number of uncles in a block with given block number.
	#[method(name = "eth_getUncleCountByBlockNumber")]
	fn block_uncles_count_by_number(&self, block_number: BlockNumber) -> RpcResult<U256>;

	/// Returns the code at given address at given time (block number).
	#[method(name = "eth_getCode")]
	fn code_at(&self, address: H160, block_number: Option<BlockNumber>) -> RpcResult<Bytes>;

	/// Sends transaction; will block waiting for signer to return the
	/// transaction hash.
	#[method(name = "eth_sendTransaction")]
	async fn send_transaction(&self, tx: TransactionRequest) -> RpcResult<H256>;

	/// Sends signed transaction, returning its hash.
	#[method(name = "eth_sendRawTransaction")]
	async fn send_raw_transaction(&self, bytes: Bytes) -> RpcResult<H256>;

	/// Call contract, returning the output data.
	#[method(name = "eth_call")]
	fn call(&self, req: CallRequest, block_number: Option<BlockNumber>) -> RpcResult<Bytes>;

	/// Estimate gas needed for execution of given contract.
	#[method(name = "eth_estimateGas")]
	fn estimate_gas(&self, req: CallRequest, block_number: Option<BlockNumber>) -> RpcResult<U256>;

	/// Get transaction by its hash.
	#[method(name = "eth_getTransactionByHash")]
	fn transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>>;

	/// Returns transaction at given block hash and index.
	#[method(name = "eth_getTransactionByBlockHashAndIndex")]
	fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> RpcResult<Option<Transaction>>;

	/// Returns transaction by given block number and index.
	#[method(name = "eth_getTransactionByBlockNumberAndIndex")]
	fn transaction_by_block_number_and_index(
		&self,
		block_number: BlockNumber,
		index: Index,
	) -> RpcResult<Option<Transaction>>;

	/// Returns transaction receipt by transaction hash.
	#[method(name = "eth_getTransactionReceipt")]
	fn transaction_receipt(&self, hash: H256) -> RpcResult<Option<Receipt>>;

	/// Returns an uncles at given block and index.
	#[method(name = "eth_getUncleByBlockHashAndIndex")]
	fn uncle_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> RpcResult<Option<RichBlock>>;

	/// Returns an uncles at given block and index.
	#[method(name = "eth_getUncleByBlockNumberAndIndex")]
	fn uncle_by_block_number_and_index(
		&self,
		block_number: BlockNumber,
		index: Index,
	) -> RpcResult<Option<RichBlock>>;

	/// Returns logs matching given filter object.
	#[method(name = "eth_getLogs")]
	fn logs(&self, filter: Filter) -> RpcResult<Vec<Log>>;

	/// Returns the hash of the current block, the seedHash, and the boundary condition to be met.
	#[method(name = "eth_getWork")]
	fn work(&self) -> RpcResult<Work>;

	/// Used for submitting a proof-of-work solution.
	#[method(name = "eth_submitWork")]
	fn submit_work(&self, h64: H64, hash: H256, hash2: H256) -> RpcResult<bool>;

	/// Used for submitting mining hashrate.
	#[method(name = "eth_submitHashrate")]
	fn submit_hashrate(&self, u256: U256, hash: H256) -> RpcResult<bool>;
}

/// Eth filters rpc api (polling).
#[rpc(server)]
pub trait EthFilterApi {
	/// Returns id of new filter.
	#[method(name = "eth_newFilter")]
	fn new_filter(&self, filter: Filter) -> RpcResult<U256>;

	/// Returns id of new block filter.
	#[method(name = "eth_newBlockFilter")]
	fn new_block_filter(&self) -> RpcResult<U256>;

	/// Returns id of new block filter.
	#[method(name = "eth_newPendingTransactionFilter")]
	fn new_pending_transaction_filter(&self) -> RpcResult<U256>;

	/// Returns filter changes since last poll.
	#[method(name = "eth_getFilterChanges")]
	fn filter_changes(&self, index: Index) -> RpcResult<FilterChanges>;

	/// Returns all logs matching given filter (in a range 'from' - 'to').
	#[method(name = "eth_getFilterLogs")]
	fn filter_logs(&self, index: Index) -> RpcResult<Vec<Log>>;

	/// Uninstalls filter.
	#[method(name = "eth_uninstallFilter")]
	fn uninstall_filter(&self, index: Index) -> RpcResult<bool>;
}
