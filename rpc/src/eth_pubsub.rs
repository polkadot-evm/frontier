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
	Rich, Header, Bytes,
	pubsub::{Kind, Params, Result as PubSubResult}
};
use ethereum_types::{H256, U256};
use codec::Decode;

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
				unimplemented!(); // TODO
			},
			Kind::NewHeads => {
				let key: StorageKey = StorageKey(
					storage_prefix_build(b"Ethereum", b"LatestHeader")
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
							let (block_hash, eth_block_header): (
								H256, ethereum::Header
							) = Decode::decode(&mut &data.0[..]).unwrap();
							return Ok::<_, ()>(Ok(
								PubSubResult::Header(Box::new(
									Rich {
										inner: Header {
											hash: Some(block_hash),
											parent_hash: eth_block_header.parent_hash,
											uncles_hash: eth_block_header.ommers_hash,
											author: eth_block_header.beneficiary,
											miner: eth_block_header.beneficiary,
											state_root: eth_block_header.state_root,
											transactions_root: eth_block_header.transactions_root,
											receipts_root: eth_block_header.receipts_root,
											number: Some(eth_block_header.number),
											gas_used: eth_block_header.gas_used,
											gas_limit: eth_block_header.gas_limit,
											extra_data: Bytes(eth_block_header.extra_data.as_bytes().to_vec()),
											logs_bloom: eth_block_header.logs_bloom,
											timestamp: U256::from(eth_block_header.timestamp),
											difficulty: eth_block_header.difficulty,
											seal_fields:  vec![
												Bytes(eth_block_header.mix_hash.as_bytes().to_vec()),
												Bytes(eth_block_header.nonce.as_bytes().to_vec())
											],
											size: Some(U256::from(0)), // TODO
										},
										extra_info: BTreeMap::new()
									}
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
			Kind::NewPendingTransactions => {
				unimplemented!(); // TODO
			},
			Kind::Syncing => {
				unimplemented!(); // TODO
			},
		}
	}

	fn unsubscribe(
		&self,
		_metadata: Option<Self::Metadata>,
		_subscription_id: SubscriptionId
	) -> JsonRpcResult<bool> {
		Ok(true)
	}
}
