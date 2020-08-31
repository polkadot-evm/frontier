use std::{marker::PhantomData, sync::Arc};
use std::collections::BTreeMap;
use sp_runtime::traits::{
	Block as BlockT, Header as _, BlakeTwo256,
	UniqueSaturatedInto
};
use sp_transaction_pool::TransactionPool;
use sp_api::{ProvideRuntimeApi, BlockId};
use sp_blockchain::HeaderBackend;
use sp_storage::StorageKey;
use sp_io::hashing::twox_128;
use sc_client_api::{
	backend::{StorageProvider, Backend, StateBackend, AuxStore},
	client::BlockchainEvents
};
use sc_rpc::Metadata;
use sp_consensus::SelectChain;
use log::warn;

use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId, manager::SubscriptionManager};
use frontier_rpc_core::EthPubSubApi::{self as EthPubSubApiT};
use frontier_rpc_core::types::{
	BlockNumber, Rich, Header, Bytes, Log, FilterAddress, Topic, VariadicValue,
	pubsub::{Kind, Params, Result as PubSubResult}
};
use ethereum_types::{H256, U256};
use codec::Decode;
use sha3::{Keccak256, Digest};

pub use frontier_rpc_core::EthPubSubApiServer;
use futures::{StreamExt as _, TryStreamExt as _};

use jsonrpc_core::{Result as JsonRpcResult, futures::{Future, Sink}, ErrorCode, Error};
use frontier_rpc_primitives::{EthereumRuntimeRPCApi, TransactionStatus};

fn internal_err(message: &str) -> Error {
	Error {
		code: ErrorCode::InternalError,
		message: message.to_string(),
		data: None
	}
}

pub struct EthPubSubApi<B: BlockT, P, C, BE, SC> {
	_pool: Arc<P>,
	client: Arc<C>,
	select_chain: SC,
	subscriptions: SubscriptionManager,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, P, C, BE, SC> EthPubSubApi<B, P, C, BE, SC> {
	pub fn new(
		_pool: Arc<P>,
		client: Arc<C>,
		select_chain: SC,
		subscriptions: SubscriptionManager,
	) -> Self {
		Self { _pool, client, select_chain, subscriptions, _marker: PhantomData }
	}
}

impl<B: BlockT, P, C, BE, SC> EthPubSubApi<B, P, C, BE, SC> where
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B,BE> +
		BlockchainEvents<B> + HeaderBackend<B> + AuxStore,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	SC: SelectChain<B> + Clone + 'static,
{
	fn native_block_number(&self, number: Option<BlockNumber>) -> JsonRpcResult<Option<u32>> {
		let header = self
			.select_chain
			.best_chain()
			.map_err(|_| internal_err("fetch header failed"))?;

		let mut native_number: Option<u32> = None;

		if let Some(number) = number {
			match number {
				BlockNumber::Hash { hash, .. } => {
					let id = match frontier_consensus::load_block_hash::<B, _>(self.client.as_ref(), hash)
						.map_err(|_| internal_err("fetch aux store failed"))?
					{
						Some(hash) => BlockId::Hash(hash),
						None => return Ok(None),
					};
					if let Ok(Some(block)) = self.client.runtime_api().current_block(&id) {
						native_number = Some(block.header.number.as_u32());
					}
				},
				BlockNumber::Num(_) => {
					if let Some(number) = number.to_min_block_num() {
						native_number = Some(number.unique_saturated_into());
					}
				},
				BlockNumber::Latest => {
					native_number = Some(
						header.number().clone().unique_saturated_into() as u32
					);
				},
				BlockNumber::Earliest => {
					native_number = Some(0);
				},
				BlockNumber::Pending => {
					native_number = None;
				}
			};
		} else {
			native_number = Some(
				header.number().clone().unique_saturated_into() as u32
			);
		}
		Ok(native_number)
	}

	fn filter(&self, params: Option<Params>) -> FilteredParams {
		let mut from_block: Option<u32> = None;
		let mut to_block: Option<u32> = None;
		let mut block_hash: Option<H256> = None;
		let mut address: Option<FilterAddress> = None;
		let mut topics: Option<Topic> = None;
		if let Some(Params::Logs(f)) = params {
			from_block = self.native_block_number(f.from_block).unwrap_or(None);
			to_block = self.native_block_number(f.to_block).unwrap_or(None);
			block_hash = f.block_hash;
			address = f.address;
			topics = f.topics;
		}
		FilteredParams {
			from_block,
			to_block,
			block_hash,
			address,
			topics
		}
	}
}

struct FilteredParams {
	from_block: Option<u32>,
	to_block: Option<u32>,
	block_hash: Option<H256>,
	address: Option<FilterAddress>,
	topics: Option<Topic>
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn new_heads_result(
	block: ethereum::Block
) -> PubSubResult {
	PubSubResult::Header(Box::new(
		Rich {
			inner: Header {
				hash: Some(H256::from_slice(Keccak256::digest(
					&rlp::encode(&block.header)
				).as_slice())),
				parent_hash: block.header.parent_hash,
				uncles_hash: block.header.ommers_hash,
				author: block.header.beneficiary,
				miner: block.header.beneficiary,
				state_root: block.header.state_root,
				transactions_root: block.header.transactions_root,
				receipts_root: block.header.receipts_root,
				number: Some(block.header.number),
				gas_used: block.header.gas_used,
				gas_limit: block.header.gas_limit,
				extra_data: Bytes(block.header.extra_data.as_bytes().to_vec()),
				logs_bloom: block.header.logs_bloom,
				timestamp: U256::from(block.header.timestamp),
				difficulty: block.header.difficulty,
				seal_fields:  vec![
					Bytes(block.header.mix_hash.as_bytes().to_vec()),
					Bytes(block.header.nonce.as_bytes().to_vec())
				],
				size: Some(U256::from(rlp::encode(&block).len() as u32)),
			},
			extra_info: BTreeMap::new()
		}
	))
}

fn logs_result(
	block: ethereum::Block,
	receipts: Vec<ethereum::Receipt>,
	params: &FilteredParams
) -> Vec<Log> {
	let block_hash = Some(H256::from_slice(
		Keccak256::digest(&rlp::encode(
			&block.header
		)).as_slice()
	));
	let mut logs: Vec<Log> = vec![];
	let mut log_index: u32 = 0;
	for receipt in receipts {
		let mut transaction_log_index: u32 = 0;
		for log in receipt.logs {
			if add_log(block_hash.unwrap(), &log, &block, params) {
				logs.push(Log {
					address: log.address,
					topics: log.topics,
					data: Bytes(log.data),
					block_hash: block_hash,
					block_number: Some(block.header.number),
					transaction_hash: Some(H256::from_slice(
						Keccak256::digest(&rlp::encode(
							&block.transactions[log_index as usize]
						)).as_slice()
					)),
					transaction_index: Some(U256::from(log_index)),
					log_index: Some(U256::from(log_index)),
					transaction_log_index: Some(U256::from(transaction_log_index)),
					removed: false,
				});
			}
			log_index += 1;
			transaction_log_index += 1;
		}
	}
	logs
}

fn filter_block_range(
	block: &ethereum::Block,
	params: &FilteredParams
) -> bool {
	let mut out = true;
	let number: u32 = UniqueSaturatedInto::<u32>::unique_saturated_into(
		block.header.number
	);
	if let Some(from) = params.from_block {
		if from > number {
			out = false;
		}
	}
	if let Some(to) = params.to_block {
		if to < number {
			out = false;
		}
	}
	out
}

fn filter_block_hash(
	block_hash: H256,
	params: &FilteredParams
) -> bool {
	if let Some(h) = params.block_hash {
		if h != block_hash { return false; }
	}
	true
}

fn filter_address(
	log: &ethereum::Log,
	params: &FilteredParams
) -> bool {
	if let Some(input_address) = &params.address {
		match input_address {
			VariadicValue::Single(x) => {
				if log.address != *x { return false; }
			},
			VariadicValue::Multiple(x) => {
				if !x.contains(&log.address) { return false; }
			},
			_ => { return true; }
		}
	}
	true
}

fn filter_topics(
	log: &ethereum::Log,
	params: &FilteredParams
) -> bool {
	if let Some(input_topics) = &params.topics {
		match input_topics {
			VariadicValue::Single(x) => {
				if !log.topics.starts_with(&vec![*x]) { return false; }
			},
			VariadicValue::Multiple(x) => {
				if !log.topics.starts_with(&x) { return false; }
			},
			_ => { return true; }
		}
	}
	true
}

fn add_log(
	block_hash: H256,
	log: &ethereum::Log,
	block: &ethereum::Block,
	params: &FilteredParams
) -> bool {
	if !filter_block_range(block, params) || !filter_block_hash(block_hash, params) ||
		!filter_address(log, params) || !filter_topics(log, params) {
		return false;
	}
	true
}

macro_rules! stream_build {
	($context:expr => $module:expr, $storage:expr) => {{
		let key: StorageKey = StorageKey(
			storage_prefix_build($module, $storage)
		);
		match $context.client.storage_changes_notification_stream(
			Some(&[key]),
			None
		) {
			Ok(stream) => Some(stream),
			Err(_err) => None,
		}
	}};
}

impl<B: BlockT, P, C, BE, SC> EthPubSubApiT for EthPubSubApi<B, P, C, BE, SC>
	where
		B: BlockT<Hash=H256> + Send + Sync + 'static,
		P: TransactionPool<Block=B> + Send + Sync + 'static,
		C: ProvideRuntimeApi<B> + StorageProvider<B,BE> +
			BlockchainEvents<B> + HeaderBackend<B> + AuxStore,
		C: Send + Sync + 'static,
		C::Api: EthereumRuntimeRPCApi<B>,
		BE: Backend<B> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
		SC: SelectChain<B> + Clone + 'static,
{
	type Metadata = Metadata;
	fn subscribe(
		&self,
		_metadata: Self::Metadata,
		subscriber: Subscriber<PubSubResult>,
		kind: Kind,
		params: Option<Params>,
	) {
		let filtered_params = self.filter(params);
		let client = self.client.clone();
		match kind {
			Kind::Logs => {
				if let Some(stream) = stream_build!(self => b"Ethereum", b"CurrentReceipts") {
					self.subscriptions.add(subscriber, |sink| {
						let stream = stream
							.flat_map(move |(block_hash, changes)| {
								let id = BlockId::Hash(block_hash);
								let data = changes.iter().last().unwrap().2.unwrap();
								let receipts: Vec<ethereum::Receipt> =
									Decode::decode(&mut &data.0[..]).unwrap();
								let block: ethereum::Block =
									client.runtime_api().current_block(&id).unwrap().unwrap();
								futures::stream::iter(logs_result(block, receipts, &filtered_params))
							})
							.map(|x| {
								return Ok::<Result<PubSubResult, jsonrpc_core::types::error::Error>, ()>(Ok(
									PubSubResult::Log(Box::new(x))
								));
							})
							.compat();

						sink
							.sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
							.send_all(stream)
							.map(|_| ())
					});
				}
			},
			Kind::NewHeads => {
				if let Some(stream) = stream_build!(self => b"Ethereum", b"CurrentBlock") {
					self.subscriptions.add(subscriber, |sink| {
						let stream = stream
							.map(|(_block, changes)| {
								let data = changes.iter().last().unwrap().2.unwrap();
								let block: ethereum::Block =
									Decode::decode(&mut &data.0[..]).unwrap();
								return Ok::<_, ()>(Ok(
									new_heads_result(block)
								));
							})
							.compat();
						sink
							.sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
							.send_all(stream)
							.map(|_| ())
					});
				}
			},
			Kind::NewPendingTransactions => {
				if let Some(stream) = stream_build!(self => b"Ethereum", b"Pending") {
					self.subscriptions.add(subscriber, |sink| {
						let stream = stream
							.flat_map(|(_block, changes)| {
								let data = changes.iter().last().unwrap().2.unwrap();
								let storage: Vec<(
									ethereum::Transaction, TransactionStatus, ethereum::Receipt
								)> = Decode::decode(&mut &data.0[..]).unwrap();
								let transactions: Vec<ethereum::Transaction> =
									storage.iter().map(|x| x.0.clone()).collect();
								futures::stream::iter(transactions)
							})
							.map(|transaction| {
								return Ok::<Result<PubSubResult, jsonrpc_core::types::error::Error>, ()>(Ok(
									PubSubResult::TransactionHash(H256::from_slice(
										Keccak256::digest(
											&rlp::encode(&transaction)
										).as_slice()
									))
								));
							})
							.compat();

						sink
							.sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
							.send_all(stream)
							.map(|_| ())
					});
				}
			},
			Kind::Syncing => {
				unimplemented!(); // TODO
			},
		}
	}

	fn unsubscribe(
		&self,
		_metadata: Option<Self::Metadata>,
		subscription_id: SubscriptionId
	) -> JsonRpcResult<bool> {
		Ok(self.subscriptions.cancel(subscription_id))
	}
}
