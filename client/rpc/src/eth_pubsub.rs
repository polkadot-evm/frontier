// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
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

use sc_client_api::{
	backend::{Backend, StateBackend, StorageProvider},
	client::BlockchainEvents,
};
use sc_rpc::SubscriptionTaskExecutor;
use sc_transaction_pool_api::TransactionPool;
use sp_api::{BlockId, ProvideRuntimeApi};
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::{BlakeTwo256, Block as BlockT, UniqueSaturatedInto};
use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use ethereum::BlockV2 as EthereumBlock;
use ethereum_types::{H256, U256};
use fc_rpc_core::{
	types::{
		pubsub::{Kind, Params, PubSubSyncStatus, Result as PubSubResult},
		Bytes, FilteredParams, Header, Log, Rich,
	},
	EthPubSubApiServer as EthPubSubApiT,
};
use jsonrpsee::types::Error;
use sha3::{Digest, Keccak256};

pub use fc_rpc_core::EthPubSubApiServer;
use futures::StreamExt as _;

use fp_rpc::EthereumRuntimeRPCApi;
use jsonrpsee::types::RpcResult;
use jsonrpsee::ws_server::SubscriptionSink;

use sc_network::{ExHashT, NetworkService};

use crate::{frontier_backend_client, overrides::OverrideHandle};

/*#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct HexEncodedIdProvider {
	len: usize,
}

impl Default for HexEncodedIdProvider {
	fn default() -> Self {
		Self { len: 16 }
	}
}

impl IdProvider for HexEncodedIdProvider {
	type Id = String;
	fn next_id(&self) -> Self::Id {
		let mut rng = thread_rng();
		let id: String = iter::repeat(())
			.map(|()| rng.sample(Alphanumeric))
			.take(self.len)
			.collect();
		let out: String = id.as_bytes().to_hex();
		format!("0x{}", out)
	}
}*/

pub struct EthPubSubApi<B: BlockT, P, C, BE, H: ExHashT> {
	pool: Arc<P>,
	client: Arc<C>,
	network: Arc<NetworkService<B, H>>,
	subscriptions: SubscriptionTaskExecutor,
	overrides: Arc<OverrideHandle<B>>,
	_marker: PhantomData<(B, BE)>,
}

impl<B: BlockT, P, C, BE, H: ExHashT> EthPubSubApi<B, P, C, BE, H>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: Send + Sync + 'static,
{
	pub fn new(
		pool: Arc<P>,
		client: Arc<C>,
		network: Arc<NetworkService<B, H>>,
		subscriptions: SubscriptionTaskExecutor,
		overrides: Arc<OverrideHandle<B>>,
	) -> Self {
		Self {
			pool: pool.clone(),
			client: client.clone(),
			network,
			subscriptions,
			overrides,
			_marker: PhantomData,
		}
	}
}

struct SubscriptionResult {}
impl SubscriptionResult {
	pub fn new() -> Self {
		SubscriptionResult {}
	}
	pub fn new_heads(&self, block: EthereumBlock) -> PubSubResult {
		PubSubResult::Header(Box::new(Rich {
			inner: Header {
				hash: Some(H256::from_slice(
					Keccak256::digest(&rlp::encode(&block.header)).as_slice(),
				)),
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
				extra_data: Bytes(block.header.extra_data.clone()),
				logs_bloom: block.header.logs_bloom,
				timestamp: U256::from(block.header.timestamp),
				difficulty: block.header.difficulty,
				seal_fields: vec![
					Bytes(block.header.mix_hash.as_bytes().to_vec()),
					Bytes(block.header.nonce.as_bytes().to_vec()),
				],
				size: Some(U256::from(rlp::encode(&block).len() as u32)),
			},
			extra_info: BTreeMap::new(),
		}))
	}
	pub fn logs(
		&self,
		block: EthereumBlock,
		receipts: Vec<ethereum::Receipt>,
		params: &FilteredParams,
	) -> Vec<Log> {
		let block_hash = Some(H256::from_slice(
			Keccak256::digest(&rlp::encode(&block.header)).as_slice(),
		));
		let mut logs: Vec<Log> = vec![];
		let mut log_index: u32 = 0;
		for (receipt_index, receipt) in receipts.into_iter().enumerate() {
			let mut transaction_log_index: u32 = 0;
			let transaction_hash: Option<H256> = if receipt.logs.len() > 0 {
				Some(block.transactions[receipt_index as usize].hash())
			} else {
				None
			};
			for log in receipt.logs {
				if self.add_log(block_hash.unwrap(), &log, &block, params) {
					logs.push(Log {
						address: log.address,
						topics: log.topics,
						data: Bytes(log.data),
						block_hash: block_hash,
						block_number: Some(block.header.number),
						transaction_hash: transaction_hash,
						transaction_index: Some(U256::from(receipt_index)),
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
	fn add_log(
		&self,
		block_hash: H256,
		ethereum_log: &ethereum::Log,
		block: &EthereumBlock,
		params: &FilteredParams,
	) -> bool {
		let log = Log {
			address: ethereum_log.address.clone(),
			topics: ethereum_log.topics.clone(),
			data: Bytes(ethereum_log.data.clone()),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		if let Some(_) = params.filter {
			let block_number =
				UniqueSaturatedInto::<u64>::unique_saturated_into(block.header.number);
			if !params.filter_block_range(block_number)
				|| !params.filter_block_hash(block_hash)
				|| !params.filter_address(&log)
				|| !params.filter_topics(&log)
			{
				return false;
			}
		}
		true
	}
}

impl<B: BlockT, P, C, BE, H: ExHashT> EthPubSubApiT for EthPubSubApi<B, P, C, BE, H>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	P: TransactionPool<Block = B> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE> + BlockchainEvents<B>,
	C: HeaderBackend<B> + HeaderMetadata<B, Error = BlockChainError> + 'static,
	C: Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	fn subscribe(
		&self,
		sink: SubscriptionSink,
		kind: Kind,
		params: Option<Params>,
	) -> RpcResult<()> {
		let filtered_params = match params {
			Some(Params::Logs(filter)) => FilteredParams::new(Some(filter)),
			_ => FilteredParams::default(),
		};

		let client = self.client.clone();
		let pool = self.pool.clone();
		let network = self.network.clone();
		let overrides = self.overrides.clone();
		match kind {
			Kind::Logs => {
				let stream = client
					.import_notification_stream()
					.filter_map(move |notification| {
						if notification.is_new_best {
							let id = BlockId::Hash(notification.hash);

							let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
								client.as_ref(),
								id,
							);
							let handler = overrides
								.schemas
								.get(&schema)
								.unwrap_or(&overrides.fallback);

							let block = handler.current_block(&id);
							let receipts = handler.current_receipts(&id);

							match (receipts, block) {
								(Some(receipts), Some(block)) => {
									futures::future::ready(Some((block, receipts)))
								}
								_ => futures::future::ready(None),
							}
						} else {
							futures::future::ready(None)
						}
					})
					.flat_map(move |(block, receipts)| {
						futures::stream::iter(SubscriptionResult::new().logs(
							block,
							receipts,
							&filtered_params,
						))
					})
					.map(|x| PubSubResult::Log(Box::new(x)));

				subscribe_helper(&self.subscriptions, sink, stream)
			}
			Kind::NewHeads => {
				let stream = client
					.import_notification_stream()
					.filter_map(move |notification| {
						if notification.is_new_best {
							let id = BlockId::Hash(notification.hash);

							let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(
								client.as_ref(),
								id,
							);
							let handler = overrides
								.schemas
								.get(&schema)
								.unwrap_or(&overrides.fallback);

							let block = handler.current_block(&id);
							futures::future::ready(block)
						} else {
							futures::future::ready(None)
						}
					})
					.map(|block| SubscriptionResult::new().new_heads(block));

				subscribe_helper(&self.subscriptions, sink, stream)
			}
			Kind::NewPendingTransactions => {
				use sc_transaction_pool_api::InPoolTransaction;

				let stream = pool
					.import_notification_stream()
					.filter_map(move |txhash| {
						if let Some(xt) = pool.ready_transaction(&txhash) {
							let best_block: BlockId<B> = BlockId::Hash(client.info().best_hash);
							let res = match client
								.runtime_api()
								.extrinsic_filter(&best_block, vec![xt.data().clone()])
							{
								Ok(txs) => {
									if txs.len() == 1 {
										Some(txs[0].clone())
									} else {
										None
									}
								}
								_ => None,
							};
							futures::future::ready(res)
						} else {
							futures::future::ready(None)
						}
					})
					.map(|transaction| PubSubResult::TransactionHash(transaction.hash()));

				subscribe_helper(&self.subscriptions, sink, stream)
			}
			Kind::Syncing => {
				let mut previous_syncing = network.is_major_syncing();
				let stream = client
					.import_notification_stream()
					.filter_map(move |notification| {
						let syncing = network.is_major_syncing();
						if notification.is_new_best && previous_syncing != syncing {
							previous_syncing = syncing;
							futures::future::ready(Some(syncing))
						} else {
							futures::future::ready(None)
						}
					})
					.map(|syncing| PubSubResult::SyncState(PubSubSyncStatus { syncing: syncing }));

				subscribe_helper(&self.subscriptions, sink, stream)
			}
		}
	}
}

fn subscribe_helper<S, Item>(
	executor: &SubscriptionTaskExecutor,
	mut sink: SubscriptionSink,
	stream: S,
) -> RpcResult<()>
where
	S: futures::stream::Stream<Item = Item> + Send + 'static,
	Item: Send + sp_runtime::Serialize + 'static,
{
	let fut = async move {
		stream
			.take_while(|item| {
				std::future::ready(sink.send(&item).map_or_else(
					|e| {
						log::debug!(
							"Could not send data to subscription: eth_subscription error: {:?}",
							e
						);
						false
					},
					|_| true,
				))
			})
			.for_each(|_| std::future::ready(()))
			.await;
	};

	use futures::task::Spawn as _;
	executor
		.spawn_obj(Box::pin(fut).into())
		.map_err(|e| Error::Custom(e.to_string()))
}
