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

/// (Non-standard) Ethereum pubsub interface.
pub mod pubsub;

use ethereum_types::{Address, H256, U256, U64};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

pub use self::pubsub::*;
use crate::types::{
	access_list::AccessListResult,
	block::Block,
	block_id::{BlockNumberOrTag, BlockNumberOrTagOrHash},
	bytes::Bytes,
	fee::FeeHistoryResult,
	filter::{Filter, FilterChanges},
	index::Index,
	log::Log,
	proof::AccountProof,
	state::StateOverrides,
	sync::SyncingStatus,
	transaction::{Transaction, TransactionReceipt, TransactionRequest},
};

/// Ethereum RPC client interfaces.
pub trait EthApiClient:
	EthBlockApiClient
	+ EthClientApiClient
	+ EthExecuteApiClient
	+ EthFeeMarketApiClient
	+ EthFilterApiClient
	+ EthSignApiClient
	+ EthStateApiClient
	+ EthSubmitApiClient
	+ EthTransactionApiClient
	+ EthPubSubApiClient
{
}

impl<T> EthApiClient for T where
	T: EthBlockApiClient
		+ EthClientApiClient
		+ EthExecuteApiClient
		+ EthFeeMarketApiClient
		+ EthFilterApiClient
		+ EthSignApiClient
		+ EthStateApiClient
		+ EthSubmitApiClient
		+ EthTransactionApiClient
		+ EthPubSubApiClient
{
}

/// Ethereum RPC server interfaces.
pub trait EthApiServer:
	EthBlockApiServer
	+ EthClientApiServer
	+ EthExecuteApiServer
	+ EthFeeMarketApiServer
	+ EthFilterApiServer
	+ EthSignApiServer
	+ EthStateApiServer
	+ EthSubmitApiServer
	+ EthTransactionApiServer
	+ EthPubSubApiServer
{
}

impl<T> EthApiServer for T where
	T: EthBlockApiServer
		+ EthClientApiServer
		+ EthExecuteApiServer
		+ EthFeeMarketApiServer
		+ EthFilterApiServer
		+ EthSignApiServer
		+ EthStateApiServer
		+ EthSubmitApiServer
		+ EthTransactionApiServer
		+ EthPubSubApiServer
{
}

/// Ethereum (block) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthBlockApi {
	/// Returns information about a block by hash.
	#[method(name = "getBlockByHash")]
	async fn block_by_hash(&self, hash: H256, full: bool) -> RpcResult<Option<Block>>;

	/// Returns information about a block by number.
	#[method(name = "getBlockByNumber")]
	async fn block_by_number(
		&self,
		block: BlockNumberOrTag,
		full: bool,
	) -> RpcResult<Option<Block>>;

	/// Returns the number of transactions in a block from a block matching the given block hash.
	#[method(name = "getBlockTransactionCountByHash")]
	async fn block_transaction_count_by_hash(&self, block_hash: H256) -> RpcResult<Option<U256>>;

	/// Returns the number of transactions in a block matching the given block number.
	#[method(name = "getBlockTransactionCountByNumber")]
	async fn block_transaction_count_by_number(
		&self,
		block: BlockNumberOrTag,
	) -> RpcResult<Option<U256>>;

	/// Returns the number of uncles in a block from a block matching the given block hash.
	#[method(name = "getUncleCountByBlockHash")]
	async fn block_uncles_count_by_hash(&self, block_hash: H256) -> RpcResult<U256>;

	/// Returns the number of uncles in a block matching the given block number.
	#[method(name = "getUncleCountByBlockNumber")]
	async fn block_uncles_count_by_number(&self, block: BlockNumberOrTag) -> RpcResult<U256>;

	/// Returns the receipts of a block by number or hash.
	#[method(name = "getBlockReceipts")]
	async fn block_transaction_receipts(
		&self,
		number_or_hash: BlockNumberOrTagOrHash,
	) -> RpcResult<Option<Vec<TransactionReceipt>>>;
}

/// Ethereum (client) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthClientApi {
	/// Returns the chain ID of the current network.
	#[method(name = "chainId")]
	async fn chain_id(&self) -> RpcResult<U64>;

	/// Returns an object with data about the sync status or false.
	#[method(name = "syncing")]
	async fn syncing(&self) -> RpcResult<SyncingStatus>;

	/// Returns the client coinbase address.
	#[method(name = "coinbase")]
	async fn author(&self) -> RpcResult<Address>;

	/// Returns a list of addresses owned by client.
	#[method(name = "accounts")]
	async fn accounts(&self) -> RpcResult<Vec<Address>>;

	/// Returns the number of most recent block.
	#[method(name = "blockNumber")]
	async fn block_number(&self) -> RpcResult<U64>;
}

/// Ethereum (execute) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthExecuteApi {
	/// Executes a new message call immediately without creating a transaction on the blockchain.
	#[method(name = "call")]
	async fn call(
		&self,
		request: TransactionRequest,
		number_or_hash: Option<BlockNumberOrTagOrHash>,
		state_overrides: Option<StateOverrides>,
		// block_overrides: Option<BlockOverrides>,
	) -> RpcResult<Bytes>;

	/// Generates and returns an estimate of hou much gas is necessary to allow the transaction to complete.
	#[method(name = "estimateGas")]
	async fn estimate_gas(
		&self,
		request: TransactionRequest,
		number_or_hash: Option<BlockNumberOrTag>,
		state_overrides: Option<StateOverrides>,
	) -> RpcResult<U256>;

	/// Generates an access list for a transaction.
	#[method(name = "createAccessList")]
	async fn create_access_list(
		&self,
		request: TransactionRequest,
		number_or_hash: Option<BlockNumberOrTag>,
	) -> RpcResult<AccessListResult>;
}

/// Ethereum (fee market) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthFeeMarketApi {
	/// Returns the current price per gas in wei.
	#[method(name = "gasPrice")]
	async fn gas_price(&self) -> RpcResult<U256>;

	/// Returns the current maxPriorityFeePerGas per gas in wei, which introduced in EIP-1159.
	#[method(name = "maxPriorityFeePerGas")]
	async fn max_priority_fee_per_gas(&self) -> RpcResult<U256>;

	/// Returns transaction base fee per gas and effective priority fee per gas for the requested/supported block range.
	///
	/// Transaction fee history, which is introduced in EIP-1159.
	#[method(name = "feeHistory")]
	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumberOrTag,
		reward_percentiles: Option<Vec<f64>>,
	) -> RpcResult<FeeHistoryResult>;
}

/// Ethereum (filter) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthFilterApi {
	/// Creates a filter object, based on filter options, to notify when the state changes (logs).
	#[method(name = "newFilter")]
	async fn new_filter(&self, filter: Filter) -> RpcResult<U256>;

	/// Creates a filter in the node, to notify when a new block arrives.
	#[method(name = "newBlockFilter")]
	async fn new_block_filter(&self) -> RpcResult<U256>;

	/// Creates a filter in the node, to notify when new pending transactions arrive.
	#[method(name = "newPendingTransactionFilter")]
	async fn new_pending_transaction_filter(&self, full: Option<bool>) -> RpcResult<U256>;

	/// Uninstalls a filter with given id.
	#[method(name = "uninstallFilter")]
	async fn uninstall_filter(&self, filter_id: Index) -> RpcResult<bool>;

	/// Polling method for a filter, which returns an array of logs which occurred since last poll.
	#[method(name = "getFilterChanges")]
	async fn filter_changes(&self, filter_id: Index) -> RpcResult<FilterChanges>;

	/// Returns an array of all logs matching filter with given id.
	#[method(name = "getFilterLogs")]
	async fn filter_logs(&self, filter_id: Index) -> RpcResult<Vec<Log>>;

	/// Returns an array of all logs matching filter with given id.
	#[method(name = "getLogs")]
	async fn logs(&self, filter: Filter) -> RpcResult<Vec<Log>>;
}

/// Ethereum (sign) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthSignApi {
	/// Returns an EIP-191 signature over the provided data.
	#[method(name = "sign")]
	async fn sign(&self, address: Address, message: Bytes) -> RpcResult<Bytes>;

	/// Returns an RLP encoded transaction signed by the specified account.
	#[method(name = "signTransaction")]
	async fn sign_transaction(&self, request: TransactionRequest) -> RpcResult<Bytes>;
}

/// Ethereum (state) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthStateApi {
	/// Returns the balance of the account of given address.
	#[method(name = "getBalance")]
	async fn balance(
		&self,
		address: Address,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<U256>;

	/// Returns the value from a storage position at a given address.
	#[method(name = "getStorageAt")]
	async fn storage_at(
		&self,
		address: Address,
		slot: U256,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<H256>;

	/// Returns the number of transactions sent from an address.
	#[method(name = "getTransactionCount")]
	async fn transaction_count(
		&self,
		address: Address,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<U256>;

	/// Returns the code at a given address.
	#[method(name = "getCode")]
	async fn code(
		&self,
		address: Address,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<Bytes>;

	/// Returns the merkle proof for a given account and optionally some storage keys.
	#[method(name = "getProof")]
	async fn proof(
		&self,
		address: Address,
		storage_keys: H256,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<AccountProof>;
}

/// Ethereum (submit) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthSubmitApi {
	/// Signs and submits a transaction; will block waiting for signer to return the transaction hash.
	#[method(name = "eth_sendTransaction")]
	async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<H256>;

	/// Submits a raw signed transaction, returning its hash.
	#[method(name = "eth_sendRawTransaction")]
	async fn send_raw_transaction(&self, bytes: Bytes) -> RpcResult<H256>;
}

/// Ethereum (transaction) RPC interface.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthTransactionApi {
	/// Returns the information about a transaction requested by transaction hash.
	#[method(name = "getTransactionByHash")]
	async fn transaction_by_hash(&self, transaction_hash: H256) -> RpcResult<Option<Transaction>>;

	/// Returns information about a transaction by block hash and transaction index position.
	#[method(name = "getTransactionByBlockHashAndIndex")]
	async fn transaction_by_block_hash_and_index(
		&self,
		block_hash: H256,
		transaction_index: Index,
	) -> RpcResult<Option<Transaction>>;

	/// Returns information about a transaction by block number and transaction index position.
	#[method(name = "getTransactionByBlockNumberAndIndex")]
	async fn transaction_by_block_number_and_index(
		&self,
		block: BlockNumberOrTag,
		transaction_index: Index,
	) -> RpcResult<Option<Transaction>>;

	/// Returns the receipt of a transaction by transaction hash.
	#[method(name = "getTransactionReceipt")]
	async fn transaction_receipt(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<TransactionReceipt>>;
}
