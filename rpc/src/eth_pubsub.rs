use std::{marker::PhantomData, sync::Arc};
use sp_runtime::traits::{Block as BlockT, BlakeTwo256};
use sp_transaction_pool::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_storage::{StorageChangeSet, StorageKey};
use sp_io::hashing::{twox_128, blake2_128};
use sc_client_api::{
    backend::{StorageProvider, Backend, StateBackend},
    client::BlockchainEvents
};
use sc_rpc::Metadata;
use log::warn;

use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId, manager::SubscriptionManager};
use frontier_rpc_core::EthPubSubApi::{self as EthPubSubApiT};
use frontier_rpc_core::types::{
    Header, 
    pubsub::{Kind, Params, Result as PubSubResult}
};
use ethereum_types::H256;

pub use frontier_rpc_core::EthPubSubApiServer;
use futures::{future, StreamExt as _, TryStreamExt as _};

use jsonrpc_core::{Result as JsonRpcResult, futures::{stream, Future, Sink, Stream, future::result}};

pub struct EthPubSubApi<B: BlockT, P, C, BE> {
	pool: Arc<P>,
	client: Arc<C>,
	subscriptions: SubscriptionManager,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, P, C, BE> EthPubSubApi<B, P, C, BE> {
    pub fn new(
        pool: Arc<P>,
        client: Arc<C>,
        subscriptions: SubscriptionManager,
    ) -> Self {
        Self { pool, client, subscriptions, _marker: PhantomData }
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
	    // C::Api: EthPubSubRuntimeApi<B>,
        BE: Backend<B> + 'static,
	    BE::State: StateBackend<BlakeTwo256>,
{
    type Metadata = Metadata;
    fn subscribe(
		&self,
		metadata: Self::Metadata,
		subscriber: Subscriber<PubSubResult>,
		kind: Kind,
		params: Option<Params>,
	) {
        let key = StorageKey(storage_prefix_build(b"System", b"Number"));
        let stream = match self.client.storage_changes_notification_stream(
            Some(&[key]),
            None
        ) {
            Ok(stream) => stream,
            Err(err) => {
                let _ = subscriber.reject(unimplemented!());
                return;
            },
        };
        self.subscriptions.add(subscriber, |sink| {
            let stream = stream
                .map(|(block, changes)| Ok::<_, ()>(Ok(
                    PubSubResult::TransactionHash(H256::default())
                )))
				.compat();
            sink
                .sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
                .send_all(stream)
                // we ignore the resulting Stream (if the first stream is over we are unsubscribed)
                .map(|_| ())
        });
    }

	fn unsubscribe(
        &self,
        metadata: Option<Self::Metadata>,
        subscription_id: SubscriptionId
    ) -> JsonRpcResult<bool> {
        Ok(true)
    }
}
