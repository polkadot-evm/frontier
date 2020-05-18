use ethereum_types::{H160, H256, H64, U256, U64};
use jsonrpc_core::{BoxFuture, Result};

use frontier_rpc_core::EthApi;
use frontier_rpc_core::types::{
	BlockNumber, Bytes, CallRequest, EthAccount, Filter, Index, Log, Receipt, RichBlock,
	SyncStatus, Transaction, Work,
};

pub use frontier_rpc_core::EthApiServer;

pub struct EthRpcImpl;

impl EthApi for EthRpcImpl {
	/// Returns protocol version encoded as a string (quotes are necessary).
	fn protocol_version(&self) -> Result<String> {
		return Ok("0x54".to_string());
	}

	fn syncing(&self) -> Result<SyncStatus> {
		unimplemented!("syncing");
	}

	fn hashrate(&self) -> Result<U256> {
		unimplemented!("hashrate");
	}

	fn author(&self) -> Result<H160> {
		unimplemented!("author");
	}

	fn is_mining(&self) -> Result<bool> {
		unimplemented!("is_mining");
	}

	fn chain_id(&self) -> Result<Option<U64>> {
		unimplemented!("chain_id");
	}

	fn gas_price(&self) -> BoxFuture<U256> {
		unimplemented!("gas_price");
	}

	fn accounts(&self) -> Result<Vec<H160>> {
		unimplemented!("accounts");
	}

	fn block_number(&self) -> Result<U256> {
		unimplemented!("block_number");
	}

	fn balance(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("balance");
	}

	fn proof(&self, _: H160, _: Vec<H256>, _: Option<BlockNumber>) -> BoxFuture<EthAccount> {
		unimplemented!("proof");
	}

	fn storage_at(&self, _: H160, _: U256, _: Option<BlockNumber>) -> BoxFuture<H256> {
		unimplemented!("storage_at");
	}

	fn block_by_hash(&self, _: H256, _: bool) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("block_by_hash");
	}

	fn block_by_number(&self, _: BlockNumber, _: bool) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("block_by_number");
	}

	fn transaction_count(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("transaction_count");
	}

	fn block_transaction_count_by_hash(&self, _: H256) -> BoxFuture<Option<U256>> {
		unimplemented!("block_transaction_count_by_hash");
	}

	fn block_transaction_count_by_number(&self, _: BlockNumber) -> BoxFuture<Option<U256>> {
		unimplemented!("block_transaction_count_by_number");
	}

	fn block_uncles_count_by_hash(&self, _: H256) -> BoxFuture<Option<U256>> {
		unimplemented!("block_uncles_count_by_hash");
	}

	fn block_uncles_count_by_number(&self, _: BlockNumber) -> BoxFuture<Option<U256>> {
		unimplemented!("block_uncles_count_by_number");
	}

	fn code_at(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<Bytes> {
		unimplemented!("code_at");
	}

	fn send_raw_transaction(&self, _: Bytes) -> Result<H256> {
		unimplemented!("send_raw_transaction");
	}

	fn submit_transaction(&self, _: Bytes) -> Result<H256> {
		unimplemented!("submit_transaction");
	}

	fn call(&self, _: CallRequest, _: Option<BlockNumber>) -> BoxFuture<Bytes> {
		unimplemented!("call");
	}

	fn estimate_gas(&self, _: CallRequest, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("estimate_gas");
	}

	fn transaction_by_hash(&self, _: H256) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_hash");
	}

	fn transaction_by_block_hash_and_index(
		&self,
		_: H256,
		_: Index,
	) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_block_hash_and_index");
	}

	fn transaction_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_block_number_and_index");
	}

	fn transaction_receipt(&self, _: H256) -> BoxFuture<Option<Receipt>> {
		unimplemented!("transaction_receipt");
	}

	fn uncle_by_block_hash_and_index(&self, _: H256, _: Index) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("uncle_by_block_hash_and_index");
	}

	fn uncle_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("uncle_by_block_number_and_index");
	}

	fn compilers(&self) -> Result<Vec<String>> {
		unimplemented!("compilers");
	}

	fn compile_lll(&self, _: String) -> Result<Bytes> {
		unimplemented!("compile_lll");
	}

	fn compile_solidity(&self, _: String) -> Result<Bytes> {
		unimplemented!("compile_solidity");
	}

	fn compile_serpent(&self, _: String) -> Result<Bytes> {
		unimplemented!("compile_serpent");
	}

	fn logs(&self, _: Filter) -> BoxFuture<Vec<Log>> {
		unimplemented!("logs");
	}

	fn work(&self, _: Option<u64>) -> Result<Work> {
		unimplemented!("work");
	}

	fn submit_work(&self, _: H64, _: H256, _: H256) -> Result<bool> {
		unimplemented!("submit_work");
	}

	fn submit_hashrate(&self, _: U256, _: H256) -> Result<bool> {
		unimplemented!("submit_hashrate");
	}
}
