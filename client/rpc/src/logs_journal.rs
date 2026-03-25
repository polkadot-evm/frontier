// This file is part of Frontier.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::{
	collections::VecDeque,
	sync::{Arc, Mutex},
};

use futures::{FutureExt as _, StreamExt as _};
use tokio::sync::broadcast;
// Substrate
use sc_rpc::SubscriptionTaskExecutor;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_mapping_sync::{EthereumBlockNotification, EthereumBlockNotificationSinks};
use fc_rpc_core::types::{Filter, Log};
use fc_storage::StorageOverride;

use crate::eth::filter::filter_block_logs_with_removed;

const DEFAULT_LOGS_JOURNAL_CAPACITY: usize = 256;

#[derive(Clone, Debug)]
pub struct LogsJournalEntry {
	pub seq: u64,
	pub complete: bool,
	pub logs: Vec<Log>,
}

#[derive(Clone, Debug)]
pub enum LogsJournalError {
	CursorTooOld {
		cursor: u64,
		earliest_available: u64,
		next_cursor: u64,
	},
	IncompleteEntry {
		seq: u64,
	},
}

#[derive(Default)]
struct LogsJournalState {
	entries: VecDeque<Arc<LogsJournalEntry>>,
	next_seq: u64,
	max_entries: usize,
}

impl LogsJournalState {
	fn with_capacity(max_entries: usize) -> Self {
		Self {
			entries: VecDeque::new(),
			next_seq: 0,
			max_entries: max_entries.max(1),
		}
	}

	fn cursor(&self) -> u64 {
		self.next_seq
	}

	fn earliest_available(&self) -> u64 {
		self.entries
			.front()
			.map(|entry| entry.seq)
			.unwrap_or(self.next_seq)
	}

	fn push(&mut self, complete: bool, logs: Vec<Log>) -> Arc<LogsJournalEntry> {
		let entry = Arc::new(LogsJournalEntry {
			seq: self.next_seq,
			complete,
			logs,
		});
		self.next_seq = self.next_seq.saturating_add(1);
		self.entries.push_back(entry.clone());
		while self.entries.len() > self.max_entries {
			self.entries.pop_front();
		}
		entry
	}
}

#[derive(Clone)]
pub struct LogsJournal {
	state: Arc<Mutex<LogsJournalState>>,
	tx: broadcast::Sender<Arc<LogsJournalEntry>>,
}

impl LogsJournal {
	pub fn new<B: BlockT + 'static>(
		executor: SubscriptionTaskExecutor,
		storage_override: Arc<dyn StorageOverride<B>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<B>>,
		>,
	) -> Self {
		Self::with_capacity(
			executor,
			storage_override,
			pubsub_notification_sinks,
			DEFAULT_LOGS_JOURNAL_CAPACITY,
		)
	}

	pub fn with_capacity<B: BlockT + 'static>(
		executor: SubscriptionTaskExecutor,
		storage_override: Arc<dyn StorageOverride<B>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<B>>,
		>,
		max_entries: usize,
	) -> Self {
		let state = Arc::new(Mutex::new(LogsJournalState::with_capacity(max_entries)));
		let (tx, _) = broadcast::channel(max_entries.max(1));

		let worker_state = state.clone();
		let worker_tx = tx.clone();
		let worker_fut = async move {
			let mut had_stream = false;
			loop {
				let (inner_sink, mut notifications) =
					sc_utils::mpsc::tracing_unbounded("logs_journal_notification_stream", 100_000);
				pubsub_notification_sinks.lock().push(inner_sink);

				while let Some(notification) = notifications.next().await {
					had_stream = true;
					if !notification.is_new_best {
						continue;
					}

					let (complete, logs) =
						build_journal_payload(storage_override.as_ref(), notification);
					let entry = {
						let mut state = worker_state.lock().expect("logs journal mutex poisoned");
						state.push(complete, logs)
					};
					let _ = worker_tx.send(entry);
				}

				if had_stream {
					let entry = {
						let mut state = worker_state.lock().expect("logs journal mutex poisoned");
						state.push(false, Vec::new())
					};
					let _ = worker_tx.send(entry);
				}
			}
		}
		.boxed();

		executor.spawn("frontier-rpc-logs-journal", Some("rpc"), worker_fut);

		Self { state, tx }
	}

	pub fn cursor(&self) -> u64 {
		self.state
			.lock()
			.expect("logs journal mutex poisoned")
			.cursor()
	}

	pub fn subscribe(&self) -> broadcast::Receiver<Arc<LogsJournalEntry>> {
		self.tx.subscribe()
	}

	pub fn snapshot_since(
		&self,
		cursor: u64,
	) -> Result<(Vec<Arc<LogsJournalEntry>>, u64), LogsJournalError> {
		let state = self.state.lock().expect("logs journal mutex poisoned");
		let earliest_available = state.earliest_available();
		let next_cursor = state.cursor();

		if cursor < earliest_available {
			return Err(LogsJournalError::CursorTooOld {
				cursor,
				earliest_available,
				next_cursor,
			});
		}

		let entries = state
			.entries
			.iter()
			.filter(|entry| entry.seq >= cursor)
			.cloned()
			.collect::<Vec<_>>();
		if let Some(entry) = entries.iter().find(|entry| !entry.complete) {
			return Err(LogsJournalError::IncompleteEntry { seq: entry.seq });
		}

		Ok((entries, next_cursor))
	}
}

/// Same payload as pushed to the journal; used by `eth_subscribe("logs")` so pub-sub matches
/// `eth_getFilterChanges` / the retained journal without reading the broadcast channel.
pub(crate) fn build_journal_payload_for_subscription<B: BlockT>(
	storage_override: &dyn StorageOverride<B>,
	notification: EthereumBlockNotification<B>,
) -> (bool, Vec<Log>) {
	build_journal_payload(storage_override, notification)
}

fn build_journal_payload<B: BlockT>(
	storage_override: &dyn StorageOverride<B>,
	notification: EthereumBlockNotification<B>,
) -> (bool, Vec<Log>) {
	let mut logs = Vec::new();
	let empty_filter = Filter::default();

	if let Some(reorg_info) = notification.reorg_info.as_deref() {
		for hash in &reorg_info.retracted {
			if !append_block_logs(storage_override, &empty_filter, *hash, true, &mut logs) {
				return (false, Vec::new());
			}
		}
		for hash in reorg_info
			.enacted
			.iter()
			.chain(std::iter::once(&reorg_info.new_best))
		{
			if !append_block_logs(storage_override, &empty_filter, *hash, false, &mut logs) {
				return (false, Vec::new());
			}
		}
		return (true, logs);
	}

	if append_block_logs(
		storage_override,
		&empty_filter,
		notification.hash,
		false,
		&mut logs,
	) {
		(true, logs)
	} else {
		(false, Vec::new())
	}
}

fn append_block_logs<B: BlockT>(
	storage_override: &dyn StorageOverride<B>,
	filter: &Filter,
	block_hash: B::Hash,
	removed: bool,
	out: &mut Vec<Log>,
) -> bool {
	let Some(block) = storage_override.current_block(block_hash) else {
		return false;
	};
	let Some(statuses) = storage_override.current_transaction_statuses(block_hash) else {
		return false;
	};
	out.extend(filter_block_logs_with_removed(
		filter, block, statuses, removed,
	));
	true
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;

	use ethereum::{BlockV3, PartialHeader};
	use ethereum_types::{Bloom, H160, H256, H64, U256};
	use fp_rpc::TransactionStatus;
	use sp_runtime::{
		generic::{Block, Header},
		traits::BlakeTwo256,
		Permill,
	};

	type OpaqueBlock = Block<Header<u64, BlakeTwo256>, sp_runtime::OpaqueExtrinsic>;

	#[derive(Default)]
	struct MockStorageOverride {
		blocks: HashMap<H256, BlockV3>,
		statuses: HashMap<H256, Vec<TransactionStatus>>,
	}

	impl StorageOverride<OpaqueBlock> for MockStorageOverride {
		fn account_code_at(&self, _at: H256, _address: H160) -> Option<Vec<u8>> {
			None
		}

		fn account_storage_at(&self, _at: H256, _address: H160, _index: U256) -> Option<H256> {
			None
		}

		fn current_block(&self, at: H256) -> Option<BlockV3> {
			self.blocks.get(&at).cloned()
		}

		fn current_receipts(&self, _at: H256) -> Option<Vec<ethereum::ReceiptV4>> {
			None
		}

		fn current_transaction_statuses(&self, at: H256) -> Option<Vec<TransactionStatus>> {
			self.statuses.get(&at).cloned()
		}

		fn elasticity(&self, _at: H256) -> Option<Permill> {
			None
		}

		fn is_eip1559(&self, _at: H256) -> bool {
			false
		}
	}

	fn make_ethereum_block(seed: u64) -> ethereum::BlockV3 {
		let partial_header = PartialHeader {
			parent_hash: H256::from_low_u64_be(seed),
			beneficiary: H160::from_low_u64_be(seed),
			state_root: H256::from_low_u64_be(seed.saturating_add(1)),
			receipts_root: H256::from_low_u64_be(seed.saturating_add(2)),
			logs_bloom: Bloom::default(),
			difficulty: U256::from(seed),
			number: U256::from(seed),
			gas_limit: U256::from(seed.saturating_add(100)),
			gas_used: U256::from(seed.saturating_add(50)),
			timestamp: seed,
			extra_data: Vec::new(),
			mix_hash: H256::from_low_u64_be(seed.saturating_add(3)),
			nonce: H64::from_low_u64_be(seed),
		};
		ethereum::Block::new(partial_header, vec![], vec![])
	}

	fn make_status(topic: H256, tx_index: u32) -> TransactionStatus {
		TransactionStatus {
			transaction_hash: H256::from_low_u64_be(topic.to_low_u64_be()),
			transaction_index: tx_index,
			from: H160::repeat_byte(0x11),
			to: Some(H160::repeat_byte(0x22)),
			contract_address: None,
			logs: vec![ethereum::Log {
				address: H160::repeat_byte(0x33),
				topics: vec![topic],
				data: vec![0x01, 0x02],
			}],
			logs_bloom: Bloom::default(),
		}
	}

	#[test]
	fn snapshot_since_returns_cursor_too_old_after_eviction() {
		let journal = LogsJournal {
			state: Arc::new(Mutex::new(LogsJournalState::with_capacity(2))),
			tx: broadcast::channel(2).0,
		};

		{
			let mut state = journal.state.lock().unwrap();
			state.push(true, Vec::new());
			state.push(true, Vec::new());
			state.push(true, Vec::new());
		}

		let err = journal.snapshot_since(0).unwrap_err();
		assert!(matches!(
			err,
			LogsJournalError::CursorTooOld {
				cursor: 0,
				earliest_available: 1,
				next_cursor: 3,
			}
		));
	}

	#[test]
	fn snapshot_since_fails_closed_on_incomplete_entry() {
		let journal = LogsJournal {
			state: Arc::new(Mutex::new(LogsJournalState::with_capacity(4))),
			tx: broadcast::channel(4).0,
		};

		{
			let mut state = journal.state.lock().unwrap();
			state.push(true, Vec::new());
			state.push(false, Vec::new());
		}

		let err = journal.snapshot_since(0).unwrap_err();
		assert!(matches!(err, LogsJournalError::IncompleteEntry { seq: 1 }));
	}

	#[test]
	fn build_payload_without_reorg_marks_logs_as_not_removed() {
		let hash = H256::repeat_byte(0xAA);
		let mut storage = MockStorageOverride::default();
		storage.blocks.insert(hash, make_ethereum_block(10));
		storage
			.statuses
			.insert(hash, vec![make_status(H256::repeat_byte(0xA1), 0)]);

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash,
			reorg_info: None,
		};
		let (complete, logs) = build_journal_payload(&storage, notification);

		assert!(complete);
		assert_eq!(logs.len(), 1);
		assert!(!logs[0].removed);
		assert_eq!(logs[0].topics, vec![H256::repeat_byte(0xA1)]);
	}

	#[test]
	fn build_payload_with_reorg_orders_retracted_then_enacted_then_new_best() {
		let retracted = H256::repeat_byte(0x10);
		let enacted = H256::repeat_byte(0x20);
		let new_best = H256::repeat_byte(0x30);

		let mut storage = MockStorageOverride::default();
		storage.blocks.insert(retracted, make_ethereum_block(1));
		storage.blocks.insert(enacted, make_ethereum_block(2));
		storage.blocks.insert(new_best, make_ethereum_block(3));
		storage
			.statuses
			.insert(retracted, vec![make_status(H256::repeat_byte(0xA1), 0)]);
		storage
			.statuses
			.insert(enacted, vec![make_status(H256::repeat_byte(0xB2), 0)]);
		storage
			.statuses
			.insert(new_best, vec![make_status(H256::repeat_byte(0xC3), 0)]);

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash: new_best,
			reorg_info: Some(Arc::new(fc_mapping_sync::ReorgInfo::<OpaqueBlock> {
				common_ancestor: H256::repeat_byte(0x01),
				retracted: vec![retracted],
				enacted: vec![enacted],
				new_best,
			})),
		};
		let (complete, logs) = build_journal_payload(&storage, notification);

		assert!(complete);
		assert_eq!(logs.len(), 3);
		assert_eq!(logs[0].topics, vec![H256::repeat_byte(0xA1)]);
		assert_eq!(logs[1].topics, vec![H256::repeat_byte(0xB2)]);
		assert_eq!(logs[2].topics, vec![H256::repeat_byte(0xC3)]);
		assert!(logs[0].removed);
		assert!(!logs[1].removed);
		assert!(!logs[2].removed);
	}

	#[test]
	fn build_payload_returns_incomplete_when_reorg_data_is_missing() {
		let retracted = H256::repeat_byte(0x10);
		let enacted = H256::repeat_byte(0x20);
		let new_best = H256::repeat_byte(0x30);

		let mut storage = MockStorageOverride::default();
		storage.blocks.insert(retracted, make_ethereum_block(1));
		storage.blocks.insert(enacted, make_ethereum_block(2));
		storage
			.statuses
			.insert(retracted, vec![make_status(H256::repeat_byte(0xA1), 0)]);
		// Missing statuses for enacted hash forces an incomplete payload.

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash: new_best,
			reorg_info: Some(Arc::new(fc_mapping_sync::ReorgInfo::<OpaqueBlock> {
				common_ancestor: H256::repeat_byte(0x01),
				retracted: vec![retracted],
				enacted: vec![enacted],
				new_best,
			})),
		};
		let (complete, logs) = build_journal_payload(&storage, notification);

		assert!(!complete);
		assert!(logs.is_empty());
	}
}
