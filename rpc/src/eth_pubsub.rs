use std::{marker::PhantomData, sync::Arc};
use std::collections::BTreeMap;
use sp_runtime::traits::{Block as BlockT, BlakeTwo256};
use sp_transaction_pool::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_storage::StorageKey;
use sp_io::hashing::twox_128;
use sc_client_api::{
	backend::{StorageProvider, Backend, StateBackend},
	client::BlockchainEvents
};
use sc_rpc::Metadata;
use log::warn;

use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId, manager::SubscriptionManager};
use frontier_rpc_core::EthPubSubApi::{self as EthPubSubApiT};
use frontier_rpc_core::types::{
	Rich, Header, Bytes, Log,
	pubsub::{Kind, Params, Result as PubSubResult}
};
use ethereum_types::{H256, U256};
use codec::Decode;
use sha3::{Keccak256, Digest};

pub use frontier_rpc_core::EthPubSubApiServer;
use futures::{StreamExt as _, TryStreamExt as _};

use jsonrpc_core::{Result as JsonRpcResult, futures::{Future, Sink}};

pub struct EthPubSubApi<B: BlockT, P, C, BE> {
	_pool: Arc<P>,
	client: Arc<C>,
	subscriptions: SubscriptionManager,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, P, C, BE> EthPubSubApi<B, P, C, BE> {
	pub fn new(
		_pool: Arc<P>,
		client: Arc<C>,
		subscriptions: SubscriptionManager,
	) -> Self {
		Self { _pool, client, subscriptions, _marker: PhantomData }
	}
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn new_heads_result(
	hash: H256,
	block: ethereum::Block
) -> PubSubResult {
	PubSubResult::Header(Box::new(
		Rich {
			inner: Header {
				hash: Some(hash),
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

fn logs(
	block: ethereum::Block,
	receipts: Vec<ethereum::Receipt>
) -> Vec<Log> {
	let mut logs: Vec<Log> = vec![];
	let mut log_index: u32 = 0;
	for receipt in receipts {
		let mut transaction_log_index: u32 = 0;
		for log in receipt.logs {
			logs.push(Log {
				address: log.address,
				topics: log.topics,
				data: Bytes(log.data),
				block_hash: Some(H256::from_slice(
					Keccak256::digest(&rlp::encode(
						&block.header
					)).as_slice()
				)),
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
			log_index += 1;
			transaction_log_index += 1;
		}
	}
	logs
}

impl<B: BlockT, P, C, BE> EthPubSubApiT for EthPubSubApi<B, P, C, BE>
	where
		B: BlockT<Hash=H256> + Send + Sync + 'static,
		P: TransactionPool<Block=B> + Send + Sync + 'static,
		C: ProvideRuntimeApi<B> + StorageProvider<B,BE> +
			BlockchainEvents<B> + HeaderBackend<B>,
		C: Send + Sync + 'static,
		BE: Backend<B> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
{
	type Metadata = Metadata;
	fn subscribe(
		&self,
		_metadata: Self::Metadata,
		subscriber: Subscriber<PubSubResult>,
		kind: Kind,
		_params: Option<Params>,
	) {
		match kind {
			Kind::Logs => {
				let key: StorageKey = StorageKey(
					storage_prefix_build(b"Ethereum", b"LatestBlock")
				);
				let stream = match self.client.storage_changes_notification_stream(
					Some(&[key]),
					None
				) {
					Ok(stream) => stream,
					Err(_err) => {
						unimplemented!(); // TODO
					},
				};
				self.subscriptions.add(subscriber, |sink| {
					let stream = stream
						.flat_map(|(_block, changes)| {
							let data = changes.iter().last().unwrap().2.unwrap();
							let (_, block, receipts): (
								H256, ethereum::Block, Vec<ethereum::Receipt>
							) = Decode::decode(&mut &data.0[..]).unwrap();
							futures::stream::iter(logs(block, receipts))
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
			},
			Kind::NewHeads => {
				let key: StorageKey = StorageKey(
					storage_prefix_build(b"Ethereum", b"LatestBlock")
				);
				let stream = match self.client.storage_changes_notification_stream(
					Some(&[key]),
					None
				) {
					Ok(stream) => stream,
					Err(_err) => {
						unimplemented!(); // TODO
					},
				};
				self.subscriptions.add(subscriber, |sink| {
					let stream = stream
						.map(|(_block, changes)| {
							let data = changes.iter().last().unwrap().2.unwrap();
							let (hash, block, _): (
								H256, ethereum::Block, Vec<ethereum::Receipt>
							) = Decode::decode(&mut &data.0[..]).unwrap();
							return Ok::<_, ()>(Ok(
								new_heads_result(hash, block)
							));
						})
						.compat();
					sink
						.sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
						.send_all(stream)
						.map(|_| ())
				});
			},
			Kind::NewPendingTransactions => {
				let key: StorageKey = StorageKey(
					storage_prefix_build(b"Ethereum", b"PendingTransactionsAndReceipts")
				);
				let stream = match self.client.storage_changes_notification_stream(
					Some(&[key]),
					None
				) {
					Ok(stream) => stream,
					Err(_err) => {
						unimplemented!(); // TODO
					},
				};
				self.subscriptions.add(subscriber, |sink| {
					let stream = stream
						.flat_map(|(_block, changes)| {
							let data = changes.iter().last().unwrap().2.unwrap();
							let transactions: Vec<(
								ethereum::Transaction, ethereum::Receipt
							)> = Decode::decode(&mut &data.0[..]).unwrap();
							futures::stream::iter(transactions)
						})
						.map(|(transaction, _)| {
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
