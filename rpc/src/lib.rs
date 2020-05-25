/// EthApi service handler.
/// 
/// All calls to client.runtime.api expect a block hash reference parameter.
/// For example:
/// 
/// 	let api = self.client.runtime_api();
/// 	let at = BlockId::hash(self.client.info().best_hash);
/// 	api.min_gas_price(&at).map_err(...)
use std::sync::Arc;
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::BlockId, traits::{Block as BlockT}};
use sp_api::ProvideRuntimeApi;
use ethereum_types::{H160, H256, H64, U256, U64};
use jsonrpc_core::{Result,BoxFuture};


pub use frontier_rpc_core::EthApi;
pub use frontier_rpc_primitives::EthRuntimeApi;
use frontier_rpc_core::types::{
	BlockNumber, Bytes, CallRequest, EthAccount, Filter, Index, Log, Receipt, RichBlock,
	SyncStatus, Transaction, Work,
};
pub use frontier_rpc_core::EthApiServer;

pub struct EthHandler<C, P> {
	client: Arc<C>,
	_marker: std::marker::PhantomData<P>,
}

impl<C, P> EthHandler<C, P> {
	pub fn new(client: Arc<C>) -> Self {
		EthHandler { client, _marker: Default::default() }
	}
}

impl<C, Block> EthApi
	for EthHandler<C, Block>
where
	Block: BlockT,
	C: Send + Sync + 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	C::Api: EthRuntimeApi<Block>
{
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
	/// client.runtime.api.min_gas_price
	fn gas_price(&self) -> BoxFuture<U256> {
		unimplemented!("gas_price");
	}

	/// client.runtime.api.evm_accounts
	fn accounts(&self) -> Result<Vec<H160>> {
		unimplemented!("accounts");
	}

	/// client.runtime.api.current_block_number
	fn block_number(&self) -> Result<U256> {
		unimplemented!("block_number");
	}

	/// client.runtime.api.account_balance
	fn balance(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("balance");
	}

	fn proof(&self, _: H160, _: Vec<H256>, _: Option<BlockNumber>) -> BoxFuture<EthAccount> {
		unimplemented!("proof");
	}

	fn storage_at(&self, _: H160, _: U256, _: Option<BlockNumber>) -> BoxFuture<H256> {
		unimplemented!("storage_at");
	}

	/// client.runtime.api.block_by_hash
	fn block_by_hash(&self, _: H256, _: bool) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("block_by_hash");
	}

	/// client.runtime.api.block_by_number
	fn block_by_number(&self, _: BlockNumber, _: bool) -> BoxFuture<Option<RichBlock>> {
		unimplemented!("block_by_number");
	}

	/// client.runtime.api.address_transaction_count
	fn transaction_count(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("transaction_count");
	}

	/// client.runtime.api.transaction_count_by_hash
	fn block_transaction_count_by_hash(&self, _: H256) -> BoxFuture<Option<U256>> {
		unimplemented!("block_transaction_count_by_hash");
	}

	/// client.runtime.api.transaction_count_by_number
	fn block_transaction_count_by_number(&self, _: BlockNumber) -> BoxFuture<Option<U256>> {
		unimplemented!("block_transaction_count_by_number");
	}

	fn block_uncles_count_by_hash(&self, _: H256) -> BoxFuture<Option<U256>> {
		unimplemented!("block_uncles_count_by_hash");
	}

	fn block_uncles_count_by_number(&self, _: BlockNumber) -> BoxFuture<Option<U256>> {
		unimplemented!("block_uncles_count_by_number");
	}

	/// client.runtime.api.bytecode_from_address
	fn code_at(&self, _: H160, _: Option<BlockNumber>) -> BoxFuture<Bytes> {
		unimplemented!("code_at");
	}

	/// client.runtime.api.execute
	fn send_raw_transaction(&self, _: Bytes) -> Result<H256> {
		unimplemented!("send_raw_transaction");
	}

	/// client.runtime.api.execute
	fn submit_transaction(&self, _: Bytes) -> Result<H256> {
		unimplemented!("submit_transaction");
	}

	/// client.runtime.api.execute_call
	fn call(&self, _: CallRequest, _: Option<BlockNumber>) -> BoxFuture<Bytes> {
		unimplemented!("call");
	}

	/// client.runtime.api.virtual_call
	fn estimate_gas(&self, _: CallRequest, _: Option<BlockNumber>) -> BoxFuture<U256> {
		unimplemented!("estimate_gas");
	}

	/// client.runtime.api.transaction_by_hash
	fn transaction_by_hash(&self, _: H256) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_hash");
	}

	/// client.runtime.api.transaction_by_block_hash
	fn transaction_by_block_hash_and_index(
		&self,
		_: H256,
		_: Index,
	) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_block_hash_and_index");
	}

	/// client.runtime.api.transaction_by_block_number
	fn transaction_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> BoxFuture<Option<Transaction>> {
		unimplemented!("transaction_by_block_number_and_index");
	}

	/// client.runtime.api.transaction_receipt
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