use std::{marker::PhantomData, sync::Arc};
use std::collections::BTreeMap;
use sp_runtime::traits::{
	Block as BlockT, BlakeTwo256,
	UniqueSaturatedInto
};
use sp_transaction_pool::TransactionPool;
use sp_api::{ProvideRuntimeApi, BlockId};
use sp_blockchain::{Error as BlockChainError, HeaderMetadata, HeaderBackend};
use sp_storage::{StorageKey, StorageData};
use sp_io::hashing::twox_128;
use sc_client_api::{
	backend::{StorageProvider, Backend, StateBackend, AuxStore},
	client::BlockchainEvents
};
use sc_rpc::Metadata;
use log::warn;

use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId, manager::SubscriptionManager};
use frontier_rpc_core::EthPubSubApi::{self as EthPubSubApiT};
use frontier_rpc_core::types::{
	BlockNumber, Rich, Header, Bytes, Log, FilterAddress, Topic, VariadicValue,
	pubsub::{Kind, Params, Result as PubSubResult, PubSubSyncStatus}
};
use ethereum_types::{H256, U256};
use codec::Decode;
use sha3::{Keccak256, Digest};

pub use frontier_rpc_core::EthPubSubApiServer;
use futures::{StreamExt as _, TryStreamExt as _};

use jsonrpc_core::{Result as JsonRpcResult, futures::{Future, Sink}};
use frontier_rpc_primitives::{EthereumRuntimeRPCApi, TransactionStatus};

use sc_network::{NetworkService, ExHashT};
use crate::internal_err;

pub struct EthPubSubApi<B: BlockT, P, C, BE, H: ExHashT> {
	_pool: Arc<P>,
	client: Arc<C>,
	network: Arc<NetworkService<B, H>>,
	subscriptions: SubscriptionManager,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, P, C, BE, H: ExHashT> EthPubSubApi<B, P, C, BE, H> {
	pub fn new(
		_pool: Arc<P>,
		client: Arc<C>,
		network: Arc<NetworkService<B, H>>,
		subscriptions: SubscriptionManager,
	) -> Self {
		Self { _pool, client, network, subscriptions, _marker: PhantomData }
	}
}

impl<B: BlockT, P, C, BE, H: ExHashT> EthPubSubApi<B, P, C, BE, H> where
	B: BlockT<Hash=H256> + Send + Sync + 'static,
	P: TransactionPool<Block=B> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B,BE> +
		BlockchainEvents<B> + AuxStore,
	C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: Send + Sync + 'static,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	fn native_block_number(&self, number: Option<BlockNumber>) -> JsonRpcResult<Option<u32>> {
		let mut native_number: Option<u32> = None;

		if let Some(number) = number {
			match number {
				BlockNumber::Hash { hash, .. } => {
					let id = self.load_hash(hash).unwrap_or(None);
					if let Some(id) = id {
						if let Ok(Some(block)) = self.client.runtime_api().current_block(&id) {
							native_number = Some(block.header.number.as_u32());
						}
					}
				},
				BlockNumber::Num(_) => {
					if let Some(number) = number.to_min_block_num() {
						native_number = Some(number.unique_saturated_into());
					}
				},
				BlockNumber::Latest => {
					native_number = Some(
						self.client.info().best_number.clone().unique_saturated_into() as u32
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
				self.client.info().best_number.clone().unique_saturated_into() as u32
			);
		}
		Ok(native_number)
	}

	fn filter_block(
		&self,
		params: Option<Params>
	) -> (Option<u32>, Option<u32>) {
		if let Some(Params::Logs(f)) = params {
			let to_block: Option<u32> = if f.to_block.is_some() {
				self.native_block_number(f.to_block).unwrap_or(None)
			} else {
				None
			};
			return (
				self.native_block_number(f.from_block).unwrap_or(None),
				to_block
			);
		}
		(None, None)
	}

	// Asumes there is only one mapped canonical block in the AuxStore, otherwise something is wrong
	fn load_hash(&self, hash: H256) -> JsonRpcResult<Option<BlockId<B>>> {
		let hashes = match frontier_consensus::load_block_hash::<B, _>(self.client.as_ref(), hash)
			.map_err(|err| internal_err(format!("fetch aux store failed: {:?}", err)))?
		{
			Some(hashes) => hashes,
			None => return Ok(None),
		};
		let out: Vec<H256> = hashes.into_iter()
			.filter_map(|h| {
				if let Ok(Some(_)) = self.client.header(BlockId::Hash(h)) {
					Some(h)
				} else {
					None
				}
			}).collect();

		if out.len() == 1 {
			return Ok(Some(
				BlockId::Hash(out[0])
			));
		}
		Ok(None)
	}
}

struct FilteredParams {
	from_block: Option<u32>,
	to_block: Option<u32>,
	block_hash: Option<H256>,
	address: Option<FilterAddress>,
	topics: Option<Topic>
}

impl FilteredParams {
	pub fn new(
		params: Option<Params>,
		block_range: (Option<u32>, Option<u32>)
	) -> Self {
		if let Some(Params::Logs(d)) = params {
			return FilteredParams {
				from_block: block_range.0,
				to_block: block_range.1,
				block_hash: d.block_hash,
				address: d.address,
				topics: d.topics
			};
		}
		FilteredParams {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: None
		}
	}

	pub fn filter_block_range(
		&self,
		block: &ethereum::Block
	) -> bool {
		let mut out = true;
		let number: u32 = UniqueSaturatedInto::<u32>::unique_saturated_into(
			block.header.number
		);
		if let Some(from) = self.from_block {
			if from > number {
				out = false;
			}
		}
		if let Some(to) = self.to_block {
			if to < number {
				out = false;
			}
		}
		out
	}

	fn filter_block_hash(
		&self,
		block_hash: H256
	) -> bool {
		if let Some(h) = self.block_hash {
			if h != block_hash { return false; }
		}
		true
	}

	fn filter_address(
		&self,
		log: &ethereum::Log
	) -> bool {
		if let Some(input_address) = &self.address {
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
		&self,
		log: &ethereum::Log
	) -> bool {
		if let Some(input_topics) = &self.topics {
			match input_topics {
				VariadicValue::Single(x) => {
					if !log.topics.starts_with(&vec![*x]) {
						return false;
					}
				},
				VariadicValue::Multiple(x) => {
					if !log.topics.starts_with(&x) {
						return false;
					}
				},
				_ => { return true; }
			}
		}
		true
	}
}

struct SubscriptionResult {}
impl SubscriptionResult {
	pub fn new() -> Self { SubscriptionResult{} }
	pub fn new_heads(&self, block: ethereum::Block) -> PubSubResult {
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
					extra_data: Bytes(
						block.header.extra_data.as_bytes().to_vec()
					),
					logs_bloom: block.header.logs_bloom,
					timestamp: U256::from(block.header.timestamp),
					difficulty: block.header.difficulty,
					seal_fields:  vec![
						Bytes(
							block.header.mix_hash.as_bytes().to_vec()
						),
						Bytes(
							block.header.nonce.as_bytes().to_vec()
						)
					],
					size: Some(U256::from(
						rlp::encode(&block).len() as u32
					)),
				},
				extra_info: BTreeMap::new()
			}
		))
	}
	pub fn logs(
		&self,
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
				if self.add_log(
					block_hash.unwrap(),
					&log,
					&block,
					params
				) {
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
						transaction_log_index: Some(U256::from(
							transaction_log_index
						)),
						removed: false,
					});
				}
				log_index += 1;
				transaction_log_index += 1;
			}
		}
		logs
	}
	fn add_log(
		&self,
		block_hash: H256,
		log: &ethereum::Log,
		block: &ethereum::Block,
		params: &FilteredParams
	) -> bool {
		if !params.filter_block_range(block) ||
			!params.filter_block_hash(block_hash) ||
			!params.filter_address(log) || !params.filter_topics(log) {
			return false;
		}
		true
	}
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
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

impl<B: BlockT, P, C, BE, H: ExHashT> EthPubSubApiT for EthPubSubApi<B, P, C, BE, H>
	where
		B: BlockT<Hash=H256> + Send + Sync + 'static,
		P: TransactionPool<Block=B> + Send + Sync + 'static,
		C: ProvideRuntimeApi<B> + StorageProvider<B,BE> +
			BlockchainEvents<B> + AuxStore,
		C: HeaderBackend<B> + HeaderMetadata<B, Error=BlockChainError> + 'static,
		C: Send + Sync + 'static,
		C::Api: EthereumRuntimeRPCApi<B>,
		BE: Backend<B> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
{
	type Metadata = Metadata;
	fn subscribe(
		&self,
		_metadata: Self::Metadata,
		subscriber: Subscriber<PubSubResult>,
		kind: Kind,
		params: Option<Params>,
	) {
		let filter_block = self.filter_block(params.clone());
		let filtered_params = FilteredParams::new(params, filter_block);
		let client = self.client.clone();
		let network = self.network.clone();
		match kind {
			Kind::Logs => {
				if let Some(stream) = stream_build!(
					self => b"Ethereum", b"CurrentReceipts"
				) {
					self.subscriptions.add(subscriber, |sink| {
						let stream = stream
						.flat_map(move |(block_hash, changes)| {
							let id = BlockId::Hash(block_hash);
							let data = changes.iter().last().unwrap().2.unwrap();
							let receipts: Vec<ethereum::Receipt> =
								Decode::decode(&mut &data.0[..]).unwrap();
							let block: ethereum::Block = client.runtime_api()
								.current_block(&id).unwrap().unwrap();
							futures::stream::iter(
								SubscriptionResult::new()
									.logs(block, receipts, &filtered_params)
							)
						})
						.map(|x| {
							return Ok::<Result<
								PubSubResult,
								jsonrpc_core::types::error::Error
							>, ()>(Ok(
								PubSubResult::Log(Box::new(x))
							));
						})
						.compat();

						sink
							.sink_map_err(|e| warn!(
								"Error sending notifications: {:?}", e
							))
							.send_all(stream)
							.map(|_| ())
					});
				}
			},
			Kind::NewHeads => {
				if let Some(stream) = stream_build!(
					self => b"Ethereum", b"CurrentBlock"
				) {
					self.subscriptions.add(subscriber, |sink| {
						let stream = stream
						.map(|(_block, changes)| {
							let data = changes.iter().last().unwrap().2.unwrap();
							let block: ethereum::Block =
								Decode::decode(&mut &data.0[..]).unwrap();
							return Ok::<_, ()>(Ok(
								SubscriptionResult::new()
									.new_heads(block)
							));
						})
						.compat();

						sink
							.sink_map_err(|e| warn!(
								"Error sending notifications: {:?}", e
							))
							.send_all(stream)
							.map(|_| ())
					});
				}
			},
			Kind::NewPendingTransactions => {
				if let Some(stream) = stream_build!(
					self => b"Ethereum", b"Pending"
				) {
					self.subscriptions.add(subscriber, |sink| {
						let stream = stream
						.flat_map(|(_block, changes)| {
							let mut transactions: Vec<ethereum::Transaction> = vec![];
							let storage: Vec<Option<StorageData>> = changes.iter()
								.filter_map(|(o_sk, _k, v)| {
									if o_sk.is_none() {
										Some(v.cloned())
									} else { None }
								}).collect();
							for change in storage {
								if let Some(data) = change {
									let storage: Vec<(
										ethereum::Transaction,
										TransactionStatus,
										ethereum::Receipt
									)> = Decode::decode(&mut &data.0[..]).unwrap();
									let tmp: Vec<ethereum::Transaction> =
										storage.iter().map(|x| x.0.clone()).collect();
									transactions.extend(tmp);
								}
							}
							futures::stream::iter(transactions)
						})
						.map(|transaction| {
							return Ok::<Result<
								PubSubResult,
								jsonrpc_core::types::error::Error
							>, ()>(Ok(
								PubSubResult::TransactionHash(H256::from_slice(
									Keccak256::digest(
										&rlp::encode(&transaction)
									).as_slice()
								))
							));
						})
						.compat();

						sink
							.sink_map_err(|e| warn!(
								"Error sending notifications: {:?}", e
							))
							.send_all(stream)
							.map(|_| ())
					});
				}
			},
			Kind::Syncing => {
				if let Some(stream) = stream_build!(
					self => b"Ethereum", b"CurrentBlock"
				) {
					self.subscriptions.add(subscriber, |sink| {
						let mut previous_syncing = network.is_major_syncing();
						let stream = stream
						.filter_map(move |(_, _)| {
							let syncing = network.is_major_syncing();
							if previous_syncing != syncing {
								previous_syncing = syncing;
								futures::future::ready(Some(syncing))
							} else {
								futures::future::ready(None)
							}
						})
						.map(|syncing| {
							return Ok::<Result<
								PubSubResult,
								jsonrpc_core::types::error::Error
							>, ()>(Ok(
								PubSubResult::SyncState(PubSubSyncStatus {
									syncing: syncing
								})
							));
						})
						.compat();
						sink
							.sink_map_err(|e| warn!(
								"Error sending notifications: {:?}", e
							))
							.send_all(stream)
							.map(|_| ())

					});
				}
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
