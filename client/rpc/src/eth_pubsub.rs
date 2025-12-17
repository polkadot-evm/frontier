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

use std::{marker::PhantomData, sync::Arc};

use ethereum::TransactionV3 as EthereumTransaction;
use futures::{future, FutureExt as _, StreamExt as _};
use jsonrpsee::{core::traits::IdProvider, server::PendingSubscriptionSink};
use log::debug;
// Substrate
use sc_client_api::{
	backend::{Backend, StorageProvider},
	client::BlockchainEvents,
};
use sc_network_sync::SyncingService;
use sc_rpc::{
	utils::{BoundedVecDeque, PendingSubscription, Subscription},
	SubscriptionTaskExecutor,
};
use sc_service::config::RpcSubscriptionIdProvider;
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool, TxHash};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};
// Frontier
use fc_mapping_sync::{EthereumBlockNotification, EthereumBlockNotificationSinks};
use fc_rpc_core::{
	types::{
		pubsub::{Kind, Params, PubSubResult, PubSubSyncing, SyncingStatus},
		FilteredParams,
	},
	EthPubSubApiServer,
};
use fc_storage::StorageOverride;
use fp_rpc::EthereumRuntimeRPCApi;

#[derive(Clone, Debug)]
pub struct EthereumSubIdProvider;
impl IdProvider for EthereumSubIdProvider {
	fn next_id(&self) -> jsonrpsee::types::SubscriptionId<'static> {
		format!("0x{}", hex::encode(rand::random::<u128>().to_le_bytes())).into()
	}
}
impl RpcSubscriptionIdProvider for EthereumSubIdProvider {}

/// Eth pub-sub API implementation.
pub struct EthPubSub<B: BlockT, P, C, BE> {
	pool: Arc<P>,
	client: Arc<C>,
	sync: Arc<SyncingService<B>>,
	executor: SubscriptionTaskExecutor,
	storage_override: Arc<dyn StorageOverride<B>>,
	starting_block: u64,
	pubsub_notification_sinks: Arc<EthereumBlockNotificationSinks<EthereumBlockNotification<B>>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, P, C, BE> Clone for EthPubSub<B, P, C, BE> {
	fn clone(&self) -> Self {
		Self {
			pool: self.pool.clone(),
			client: self.client.clone(),
			sync: self.sync.clone(),
			executor: self.executor.clone(),
			storage_override: self.storage_override.clone(),
			starting_block: self.starting_block,
			pubsub_notification_sinks: self.pubsub_notification_sinks.clone(),
			_marker: PhantomData::<BE>,
		}
	}
}

impl<B: BlockT, P, C, BE> EthPubSub<B, P, C, BE>
where
	P: TransactionPool<Block = B, Hash = B::Hash> + 'static,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE>,
	BE: Backend<B> + 'static,
{
	pub fn new(
		pool: Arc<P>,
		client: Arc<C>,
		sync: Arc<SyncingService<B>>,
		executor: SubscriptionTaskExecutor,
		storage_override: Arc<dyn StorageOverride<B>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<B>>,
		>,
	) -> Self {
		// Capture the best block as seen on initialization. Used for syncing subscriptions.
		let best_number = client.info().best_number;
		let starting_block = UniqueSaturatedInto::<u64>::unique_saturated_into(best_number);
		Self {
			pool,
			client,
			sync,
			executor,
			storage_override,
			starting_block,
			pubsub_notification_sinks,
			_marker: PhantomData,
		}
	}

	/// Get headers for enacted blocks during a reorg.
	///
	/// Per Ethereum spec (https://github.com/ethereum/go-ethereum/wiki/RPC-PUB-SUB#newheads):
	/// "When a chain reorganization occurs, this subscription will emit an event
	/// containing all new headers (blocks) for the new chain. This means that you
	/// may see multiple headers emitted with the same height (block number)."
	///
	/// Returns headers in ascending order (oldest first).
	/// Note: `enacted` from tree_route already includes the new best block.
	fn get_enacted_headers(&self, enacted: &[B::Hash]) -> Vec<PubSubResult> {
		enacted
			.iter()
			.filter_map(|hash| self.storage_override.current_block(*hash))
			.map(PubSubResult::header)
			.collect()
	}

	fn notify_logs(
		&self,
		notification: EthereumBlockNotification<B>,
		params: &FilteredParams,
	) -> future::Ready<Option<impl Iterator<Item = PubSubResult>>> {
		let res = if notification.is_new_best {
			let substrate_hash = notification.hash;

			let block = self.storage_override.current_block(substrate_hash);
			let statuses = self
				.storage_override
				.current_transaction_statuses(substrate_hash);

			match (block, statuses) {
				(Some(block), Some(statuses)) => Some((block, statuses)),
				_ => None,
			}
		} else {
			None
		};

		future::ready(res.map(|(block, statuses)| {
			let logs = crate::eth::filter::filter_block_logs(&params.filter, block, statuses);

			logs.clone()
				.into_iter()
				.map(|log| PubSubResult::Log(Box::new(log.clone())))
		}))
	}

	fn pending_transactions(&self, hash: &TxHash<P>) -> future::Ready<Option<PubSubResult>> {
		let res = if let Some(xt) = self.pool.ready_transaction(hash) {
			let best_block = self.client.info().best_hash;

			let api = self.client.runtime_api();

			let api_version = if let Ok(Some(api_version)) =
				api.api_version::<dyn EthereumRuntimeRPCApi<B>>(best_block)
			{
				api_version
			} else {
				return future::ready(None);
			};

			let xts = vec![xt.data().as_ref().clone()];

			let txs: Option<Vec<EthereumTransaction>> = if api_version > 1 {
				api.extrinsic_filter(best_block, xts).ok()
			} else {
				#[allow(deprecated)]
				if let Ok(legacy) = api.extrinsic_filter_before_version_2(best_block, xts) {
					Some(legacy.into_iter().map(|tx| tx.into()).collect())
				} else {
					None
				}
			};

			match txs {
				Some(txs) => {
					if txs.len() == 1 {
						Some(txs[0].clone())
					} else {
						None
					}
				}
				_ => None,
			}
		} else {
			None
		};
		future::ready(res.map(|tx| PubSubResult::transaction_hash(&tx)))
	}

	async fn syncing_status(&self) -> PubSubSyncing {
		if self.sync.is_major_syncing() {
			// Best imported block.
			let current_number = self.client.info().best_number;
			// Get the target block to sync.
			let highest_number = self
				.sync
				.status()
				.await
				.ok()
				.and_then(|status| status.best_seen_block);

			PubSubSyncing::Syncing(SyncingStatus {
				starting_block: self.starting_block,
				current_block: UniqueSaturatedInto::<u64>::unique_saturated_into(current_number),
				highest_block: highest_number
					.map(UniqueSaturatedInto::<u64>::unique_saturated_into),
			})
		} else {
			PubSubSyncing::Synced(false)
		}
	}
}

impl<B: BlockT, P, C, BE> EthPubSubApiServer for EthPubSub<B, P, C, BE>
where
	B: BlockT,
	P: TransactionPool<Block = B, Hash = B::Hash> + 'static,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: BlockchainEvents<B> + 'static,
	C: HeaderBackend<B> + StorageProvider<B, BE>,
	BE: Backend<B> + 'static,
{
	fn subscribe(&self, pending: PendingSubscriptionSink, kind: Kind, params: Option<Params>) {
		let filtered_params = match params {
			Some(Params::Logs(filter)) => FilteredParams::new(filter),
			_ => FilteredParams::default(),
		};

		let pubsub = self.clone();
		// Everytime a new subscription is created, a new mpsc channel is added to the sink pool.
		let (inner_sink, block_notification_stream) =
			sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);
		self.pubsub_notification_sinks.lock().push(inner_sink);

		let fut = async move {
			match kind {
				Kind::NewHeads => {
					// Per Ethereum spec, when a reorg occurs, we must emit all headers
					// for the new canonical chain. The reorg_info field in the notification
					// contains the enacted blocks when a reorg occurred.
					let stream = block_notification_stream.filter_map(move |notification| {
						if !notification.is_new_best {
							return future::ready(None);
						}

						// Check if this block came from a reorg
						let headers = if let Some(ref reorg_info) = notification.reorg_info {
							debug!(
								target: "eth-pubsub",
								"Reorg detected: {} blocks retracted, {} blocks enacted",
								reorg_info.retracted.len(),
								reorg_info.enacted.len()
							);
							// Emit all enacted blocks (already includes the new best block)
							pubsub.get_enacted_headers(&reorg_info.enacted)
						} else {
							// Normal case: just emit the new block
							if let Some(block) =
								pubsub.storage_override.current_block(notification.hash)
							{
								vec![PubSubResult::header(block)]
							} else {
								return future::ready(None);
							}
						};

						if headers.is_empty() {
							return future::ready(None);
						}

						future::ready(Some(headers))
					});

					// Flatten the Vec<PubSubResult> into individual PubSubResult items
					let flat_stream = stream.flat_map(futures::stream::iter);

					PendingSubscription::from(pending)
						.pipe_from_stream(flat_stream, BoundedVecDeque::new(16))
						.await
				}
				Kind::Logs => {
					let stream = block_notification_stream
						.filter_map(move |notification| {
							pubsub.notify_logs(notification, &filtered_params)
						})
						.flat_map(futures::stream::iter);
					PendingSubscription::from(pending)
						.pipe_from_stream(stream, BoundedVecDeque::new(16))
						.await
				}
				Kind::NewPendingTransactions => {
					let pool = pubsub.pool.clone();
					let stream = pool
						.import_notification_stream()
						.filter_map(move |hash| pubsub.pending_transactions(&hash));
					PendingSubscription::from(pending)
						.pipe_from_stream(stream, BoundedVecDeque::new(16))
						.await;
				}
				Kind::Syncing => {
					let Ok(sink) = pending.accept().await else {
						return;
					};
					// On connection subscriber expects a value.
					// Because import notifications are only emitted when the node is synced or
					// in case of reorg, the first event is emitted right away.
					let syncing_status = pubsub.syncing_status().await;
					let subscription = Subscription::from(sink);
					let _ = subscription
						.send(&PubSubResult::SyncingStatus(syncing_status))
						.await;

					// When the node is not under a major syncing (i.e. from genesis), react
					// normally to import notifications.
					//
					// Only send new notifications down the pipe when the syncing status changed.
					let mut stream = pubsub.client.import_notification_stream();
					let mut last_syncing_status = pubsub.sync.is_major_syncing();
					while (stream.next().await).is_some() {
						let syncing_status = pubsub.sync.is_major_syncing();
						if syncing_status != last_syncing_status {
							let syncing_status = pubsub.syncing_status().await;
							let _ = subscription
								.send(&PubSubResult::SyncingStatus(syncing_status))
								.await;
						}
						last_syncing_status = syncing_status;
					}
				}
			}
		}
		.boxed();

		self.executor
			.spawn("frontier-rpc-subscription", Some("rpc"), fut);
	}
}
