// This file is part of Frontier.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::{
	collections::VecDeque,
	mem::size_of,
	sync::{Arc, Mutex},
	time::Duration,
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

use crate::eth::filter::visit_block_logs_with_removed;

const DEFAULT_LOGS_JOURNAL_MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;
const DEFAULT_LOGS_JOURNAL_MAX_BLOCKS_PER_ENTRY: usize = 128;
const DEFAULT_LOGS_JOURNAL_MAX_LOGS_PER_ENTRY: usize = 10_000;
const DEFAULT_LOGS_JOURNAL_MAX_BYTES_PER_ENTRY: usize = 4 * 1024 * 1024;
const MAX_BROADCAST_CAPACITY: usize = 4_096;

/// When the per-connection notification stream ends (sender dropped — e.g. sink pool refresh or
/// RPC restart), we register a new sink and continue. A short pause avoids tight spinning if sinks
/// churn rapidly; we intentionally **do not** cap retries so the journal keeps tracking the best
/// chain for the lifetime of the process.
const LOGS_JOURNAL_RECONNECT_BACKOFF: Duration = Duration::from_millis(50);

#[derive(Clone, Debug)]
pub struct LogsJournalConfig {
	pub max_entries: usize,
	pub max_total_logs: usize,
	pub max_total_bytes: usize,
	pub max_blocks_per_entry: usize,
	pub max_logs_per_entry: usize,
	pub max_bytes_per_entry: usize,
}

impl Default for LogsJournalConfig {
	fn default() -> Self {
		Self::from_max_total_bytes(DEFAULT_LOGS_JOURNAL_MAX_TOTAL_BYTES)
	}
}

impl LogsJournalConfig {
	pub fn from_max_total_bytes(max_total_bytes: usize) -> Self {
		let normalized_total_bytes = max_total_bytes.max(1);
		let max_bytes_per_entry =
			DEFAULT_LOGS_JOURNAL_MAX_BYTES_PER_ENTRY.min(normalized_total_bytes);
		let max_entries = normalized_total_bytes
			.saturating_add(max_bytes_per_entry.saturating_sub(1))
			/ max_bytes_per_entry;
		let max_logs_per_entry = DEFAULT_LOGS_JOURNAL_MAX_LOGS_PER_ENTRY;

		Self {
			max_entries: max_entries.max(1),
			max_total_logs: max_entries.max(1).saturating_mul(max_logs_per_entry),
			max_total_bytes: normalized_total_bytes,
			max_blocks_per_entry: DEFAULT_LOGS_JOURNAL_MAX_BLOCKS_PER_ENTRY,
			max_logs_per_entry,
			max_bytes_per_entry,
		}
	}

	fn normalized(&self) -> Self {
		let max_blocks_per_entry = non_zero_or_default(
			self.max_blocks_per_entry,
			DEFAULT_LOGS_JOURNAL_MAX_BLOCKS_PER_ENTRY,
		);
		let max_logs_per_entry = non_zero_or_default(
			self.max_logs_per_entry,
			DEFAULT_LOGS_JOURNAL_MAX_LOGS_PER_ENTRY,
		);
		let requested_max_bytes_per_entry = non_zero_or_default(
			self.max_bytes_per_entry,
			DEFAULT_LOGS_JOURNAL_MAX_BYTES_PER_ENTRY,
		);
		let max_total_bytes = match self.max_total_bytes {
			0 if self.max_entries > 0 => self
				.max_entries
				.saturating_mul(requested_max_bytes_per_entry),
			0 => DEFAULT_LOGS_JOURNAL_MAX_TOTAL_BYTES,
			bytes => bytes,
		}
		.max(1);
		let max_bytes_per_entry = requested_max_bytes_per_entry.min(max_total_bytes);
		let max_entries = if self.max_entries == 0 {
			max_total_bytes.saturating_add(max_bytes_per_entry.saturating_sub(1))
				/ max_bytes_per_entry
		} else {
			self.max_entries
		}
		.max(1);
		let max_total_logs = if self.max_total_logs == 0 {
			max_entries.saturating_mul(max_logs_per_entry)
		} else {
			self.max_total_logs
		}
		.max(1);

		Self {
			max_entries,
			max_total_logs,
			max_total_bytes: max_total_bytes.max(1),
			max_blocks_per_entry,
			max_logs_per_entry,
			max_bytes_per_entry,
		}
	}
}

fn non_zero_or_default(value: usize, default: usize) -> usize {
	if value == 0 {
		default.max(1)
	} else {
		value
	}
}

fn broadcast_capacity(max_entries: usize) -> usize {
	max_entries.clamp(1, MAX_BROADCAST_CAPACITY)
}

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

struct StoredEntry {
	entry: Arc<LogsJournalEntry>,
	retained_logs: usize,
	retained_bytes: usize,
}

struct LogsJournalState {
	entries: VecDeque<StoredEntry>,
	next_seq: u64,
	max_entries: usize,
	max_total_logs: usize,
	max_total_bytes: usize,
	total_logs: usize,
	total_bytes: usize,
}

impl LogsJournalState {
	fn with_config(config: &LogsJournalConfig) -> Self {
		Self {
			entries: VecDeque::new(),
			next_seq: 0,
			max_entries: config.max_entries,
			max_total_logs: config.max_total_logs,
			max_total_bytes: config.max_total_bytes,
			total_logs: 0,
			total_bytes: 0,
		}
	}

	fn cursor(&self) -> u64 {
		self.next_seq
	}

	fn earliest_available(&self) -> u64 {
		self.entries
			.front()
			.map(|entry| entry.entry.seq)
			.unwrap_or(self.next_seq)
	}

	fn push(&mut self, complete: bool, logs: Vec<Log>) -> Arc<LogsJournalEntry> {
		let retained_logs = logs.len();
		let retained_bytes = retained_entry_bytes(&logs);
		let entry = Arc::new(LogsJournalEntry {
			seq: self.next_seq,
			complete,
			logs,
		});
		self.next_seq = self.next_seq.saturating_add(1);
		self.total_logs = self.total_logs.saturating_add(retained_logs);
		self.total_bytes = self.total_bytes.saturating_add(retained_bytes);
		self.entries.push_back(StoredEntry {
			entry: entry.clone(),
			retained_logs,
			retained_bytes,
		});
		while self.entries.len() > self.max_entries
			|| self.total_logs > self.max_total_logs
			|| self.total_bytes > self.max_total_bytes
		{
			let Some(evicted) = self.entries.pop_front() else {
				break;
			};
			self.total_logs = self.total_logs.saturating_sub(evicted.retained_logs);
			self.total_bytes = self.total_bytes.saturating_sub(evicted.retained_bytes);
		}
		entry
	}
}

type SpawnLogsJournalWorker =
	Box<dyn FnOnce(Arc<Mutex<LogsJournalState>>, broadcast::Sender<Arc<LogsJournalEntry>>) + Send>;

#[derive(Clone)]
pub struct LogsJournal {
	state: Arc<Mutex<LogsJournalState>>,
	tx: broadcast::Sender<Arc<LogsJournalEntry>>,
	worker_init: Arc<Mutex<Option<SpawnLogsJournalWorker>>>,
}

impl LogsJournal {
	pub fn new<B: BlockT + 'static>(
		executor: SubscriptionTaskExecutor,
		storage_override: Arc<dyn StorageOverride<B>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<B>>,
		>,
	) -> Self {
		Self::with_config(
			executor,
			storage_override,
			pubsub_notification_sinks,
			LogsJournalConfig::default(),
		)
	}

	pub fn with_config<B: BlockT + 'static>(
		executor: SubscriptionTaskExecutor,
		storage_override: Arc<dyn StorageOverride<B>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<B>>,
		>,
		config: LogsJournalConfig,
	) -> Self {
		let config = config.normalized();
		let state = Arc::new(Mutex::new(LogsJournalState::with_config(&config)));
		let (tx, _) = broadcast::channel(broadcast_capacity(config.max_entries));
		#[rustfmt::skip]
		let worker_init = Arc::new(Mutex::new(Some(Box::new(
			move |worker_state: Arc<Mutex<LogsJournalState>>,
				worker_tx: broadcast::Sender<Arc<LogsJournalEntry>>| {
				let initial_notifications =
					register_notification_stream(&pubsub_notification_sinks);
				let worker_fut = async move {
					let mut notifications = initial_notifications;
					loop {
						while let Some(notification) = notifications.next().await {
							if !notification.is_new_best {
								continue;
							}

							let (complete, logs) = build_journal_payload(
								storage_override.as_ref(),
								notification,
								&config,
							);
							let entry = {
								let mut state =
									worker_state.lock().expect("logs journal mutex poisoned");
								state.push(complete, logs)
							};
							let _ = worker_tx.send(entry);
						}

						// Stream ended: fail closed for consumers only if we had a complete tail
						// (continuity may be broken). Skip if the journal is still empty (no
						// notifications yet) or the tail is already incomplete (do not stack
						// duplicate gap markers).
						let maybe_gap = {
							let mut state =
								worker_state.lock().expect("logs journal mutex poisoned");
							match state.entries.back() {
								Some(last) if last.entry.complete => {
									Some(state.push(false, Vec::new()))
								}
								_ => None,
							}
						};
						if let Some(entry) = maybe_gap {
							let _ = worker_tx.send(entry);
						}

						tokio::time::sleep(LOGS_JOURNAL_RECONNECT_BACKOFF).await;
						notifications = register_notification_stream(&pubsub_notification_sinks);
					}
				}
				.boxed();

				executor.spawn("frontier-rpc-logs-journal", Some("rpc"), worker_fut);
			},
		) as SpawnLogsJournalWorker)));

		Self {
			state,
			tx,
			worker_init,
		}
	}

	#[deprecated(note = "use with_config / from_max_total_bytes")]
	pub fn with_capacity<B: BlockT + 'static>(
		executor: SubscriptionTaskExecutor,
		storage_override: Arc<dyn StorageOverride<B>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<B>>,
		>,
		max_entries: usize,
	) -> Self {
		let total_bytes = max_entries
			.max(1)
			.saturating_mul(DEFAULT_LOGS_JOURNAL_MAX_BYTES_PER_ENTRY);
		Self::with_config(
			executor,
			storage_override,
			pubsub_notification_sinks,
			LogsJournalConfig::from_max_total_bytes(total_bytes),
		)
	}

	fn ensure_started(&self) {
		let spawn_worker = self
			.worker_init
			.lock()
			.expect("logs journal mutex poisoned")
			.take();
		if let Some(spawn_worker) = spawn_worker {
			spawn_worker(self.state.clone(), self.tx.clone());
		}
	}

	pub fn cursor(&self) -> u64 {
		self.ensure_started();
		self.state
			.lock()
			.expect("logs journal mutex poisoned")
			.cursor()
	}

	pub fn subscribe(&self) -> broadcast::Receiver<Arc<LogsJournalEntry>> {
		self.ensure_started();
		self.tx.subscribe()
	}

	pub fn snapshot_since(
		&self,
		cursor: u64,
	) -> Result<(Vec<Arc<LogsJournalEntry>>, u64), LogsJournalError> {
		self.ensure_started();
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
			.filter(|stored| stored.entry.seq >= cursor)
			.map(|stored| stored.entry.clone())
			.collect::<Vec<_>>();
		if let Some(entry) = entries.iter().find(|entry| !entry.complete) {
			return Err(LogsJournalError::IncompleteEntry { seq: entry.seq });
		}

		Ok((entries, next_cursor))
	}
}

fn register_notification_stream<B: BlockT>(
	pubsub_notification_sinks: &Arc<EthereumBlockNotificationSinks<EthereumBlockNotification<B>>>,
) -> sc_utils::mpsc::TracingUnboundedReceiver<EthereumBlockNotification<B>> {
	let (inner_sink, notifications) =
		sc_utils::mpsc::tracing_unbounded("logs_journal_notification_stream", 100_000);
	pubsub_notification_sinks.lock().push(inner_sink);
	notifications
}

fn build_journal_payload<B: BlockT>(
	storage_override: &dyn StorageOverride<B>,
	notification: EthereumBlockNotification<B>,
	config: &LogsJournalConfig,
) -> (bool, Vec<Log>) {
	let mut logs = Vec::new();
	let empty_filter = Filter::default();
	let mut dynamic_bytes = 0usize;

	if let Some(reorg_info) = notification.reorg_info.as_deref() {
		let total_blocks = reorg_info
			.retracted
			.len()
			.saturating_add(reorg_info.enacted.len())
			.saturating_add(1);
		if total_blocks > config.max_blocks_per_entry {
			log::debug!(
				target: "rpc",
				"Reorg journal entry spans {total_blocks} blocks, exceeding cap {}; marking incomplete",
				config.max_blocks_per_entry,
			);
			return (false, Vec::new());
		}
		for hash in &reorg_info.retracted {
			if !append_block_logs(
				storage_override,
				&empty_filter,
				*hash,
				true,
				config,
				&mut logs,
				&mut dynamic_bytes,
			) {
				return (false, Vec::new());
			}
		}
		for hash in reorg_info
			.enacted
			.iter()
			.chain(std::iter::once(&reorg_info.new_best))
		{
			if !append_block_logs(
				storage_override,
				&empty_filter,
				*hash,
				false,
				config,
				&mut logs,
				&mut dynamic_bytes,
			) {
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
		config,
		&mut logs,
		&mut dynamic_bytes,
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
	config: &LogsJournalConfig,
	out: &mut Vec<Log>,
	dynamic_bytes: &mut usize,
) -> bool {
	let Some(block) = storage_override.current_block(block_hash) else {
		log::debug!(
			target: "rpc",
			"Missing block data for {block_hash:?}, marking journal entry incomplete"
		);
		return false;
	};
	let Some(statuses) = storage_override.current_transaction_statuses(block_hash) else {
		log::debug!(
			target: "rpc",
			"Missing transaction statuses for {block_hash:?}, marking journal entry incomplete"
		);
		return false;
	};

	let mut limit_exceeded = false;
	let _ = visit_block_logs_with_removed(filter, block, statuses, removed, |log| {
		if out.len().saturating_add(1) > config.max_logs_per_entry {
			limit_exceeded = true;
			return std::ops::ControlFlow::Break(());
		}

		let log_dynamic_bytes = retained_log_dynamic_bytes(&log);
		out.push(log);
		let potential_dynamic = dynamic_bytes.saturating_add(log_dynamic_bytes);
		if retained_entry_bytes_with_capacity(out.capacity(), potential_dynamic)
			> config.max_bytes_per_entry
		{
			out.pop();
			limit_exceeded = true;
			return std::ops::ControlFlow::Break(());
		}
		*dynamic_bytes = potential_dynamic;

		std::ops::ControlFlow::Continue(())
	});

	if limit_exceeded {
		log::debug!(
			target: "rpc",
			"Journal entry for block {block_hash:?} exceeded per-entry cap (logs={}, bytes={}); marking incomplete",
			config.max_logs_per_entry,
			config.max_bytes_per_entry,
		);
		return false;
	}

	true
}

fn retained_log_dynamic_bytes(log: &Log) -> usize {
	log.topics
		.capacity()
		.saturating_mul(size_of::<ethereum_types::H256>())
		.saturating_add(log.data.0.capacity())
}

fn retained_entry_bytes_with_capacity(logs_capacity: usize, dynamic_bytes: usize) -> usize {
	size_of::<LogsJournalEntry>()
		.saturating_add(logs_capacity.saturating_mul(size_of::<Log>()))
		.saturating_add(dynamic_bytes)
}

fn retained_entry_bytes(logs: &Vec<Log>) -> usize {
	let dynamic_bytes = logs
		.iter()
		.map(retained_log_dynamic_bytes)
		.fold(0usize, usize::saturating_add);
	retained_entry_bytes_with_capacity(logs.capacity(), dynamic_bytes)
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
	use tokio::sync::broadcast::error::RecvError;

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

	/// Shared builder for mock [`TransactionStatus`] values used across journal tests.
	fn transaction_status_with_logs(
		tx_seed: u64,
		tx_index: u32,
		log_count: usize,
		data_len: usize,
	) -> TransactionStatus {
		TransactionStatus {
			transaction_hash: H256::from_low_u64_be(tx_seed),
			transaction_index: tx_index,
			from: H160::repeat_byte(0x11),
			to: Some(H160::repeat_byte(0x22)),
			contract_address: None,
			logs: (0..log_count)
				.map(|i| ethereum::Log {
					address: H160::repeat_byte(0x33),
					topics: vec![H256::from_low_u64_be(tx_seed.saturating_add(i as u64))],
					data: vec![0xAB; data_len],
				})
				.collect(),
			logs_bloom: Bloom::default(),
		}
	}

	fn make_status(topic: H256, tx_index: u32) -> TransactionStatus {
		let mut s = transaction_status_with_logs(topic.to_low_u64_be(), tx_index, 1, 2);
		s.logs[0].topics = vec![topic];
		s.logs[0].data = vec![0x01, 0x02];
		s
	}

	#[test]
	fn snapshot_since_returns_cursor_too_old_after_eviction() {
		let journal = LogsJournal {
			state: Arc::new(Mutex::new(LogsJournalState::with_config(
				&LogsJournalConfig {
					max_entries: 2,
					..LogsJournalConfig::default()
				},
			))),
			tx: broadcast::channel(2).0,
			worker_init: Arc::new(Mutex::new(None)),
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
			state: Arc::new(Mutex::new(LogsJournalState::with_config(
				&LogsJournalConfig {
					max_entries: 4,
					..LogsJournalConfig::default()
				},
			))),
			tx: broadcast::channel(4).0,
			worker_init: Arc::new(Mutex::new(None)),
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
	fn snapshot_since_empty_journal_returns_ok_with_no_entries() {
		let journal = LogsJournal {
			state: Arc::new(Mutex::new(LogsJournalState::with_config(
				&LogsJournalConfig {
					max_entries: 4,
					..LogsJournalConfig::default()
				},
			))),
			tx: broadcast::channel(4).0,
			worker_init: Arc::new(Mutex::new(None)),
		};
		let (entries, next_cursor) = journal.snapshot_since(0).unwrap();
		assert!(entries.is_empty());
		assert_eq!(next_cursor, 0);
	}

	#[test]
	fn snapshot_since_respects_cursor_excludes_older_sequences() {
		let journal = LogsJournal {
			state: Arc::new(Mutex::new(LogsJournalState::with_config(
				&LogsJournalConfig {
					max_entries: 8,
					..LogsJournalConfig::default()
				},
			))),
			tx: broadcast::channel(8).0,
			worker_init: Arc::new(Mutex::new(None)),
		};
		let mk = |topic_byte: u8| Log {
			address: H160::repeat_byte(0x01),
			topics: vec![H256::repeat_byte(topic_byte)],
			data: fc_rpc_core::types::Bytes(vec![]),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		{
			let mut state = journal.state.lock().unwrap();
			state.push(true, vec![mk(0x10)]);
			state.push(true, vec![mk(0x20)]);
			state.push(true, vec![mk(0x30)]);
		}
		let (entries, next_cursor) = journal.snapshot_since(1).unwrap();
		assert_eq!(next_cursor, 3);
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0].seq, 1);
		assert_eq!(entries[1].seq, 2);
		assert_eq!(entries[0].logs[0].topics, vec![H256::repeat_byte(0x20)]);
		assert_eq!(entries[1].logs[0].topics, vec![H256::repeat_byte(0x30)]);

		let (from_two, _) = journal.snapshot_since(2).unwrap();
		assert_eq!(from_two.len(), 1);
		assert_eq!(from_two[0].seq, 2);
	}

	#[test]
	fn snapshot_since_at_next_seq_returns_empty_snapshot() {
		let journal = LogsJournal {
			state: Arc::new(Mutex::new(LogsJournalState::with_config(
				&LogsJournalConfig {
					max_entries: 8,
					..LogsJournalConfig::default()
				},
			))),
			tx: broadcast::channel(8).0,
			worker_init: Arc::new(Mutex::new(None)),
		};
		{
			let mut state = journal.state.lock().unwrap();
			state.push(true, Vec::new());
			state.push(true, Vec::new());
			state.push(true, Vec::new());
		}
		let (entries, next_cursor) = journal.snapshot_since(3).unwrap();
		assert!(
			entries.is_empty(),
			"cursor at next_seq means nothing committed yet at or after that sequence"
		);
		assert_eq!(next_cursor, 3);
	}

	#[test]
	fn state_pushes_gap_marker_incomplete_empty_after_complete_tail() {
		let sample_log = Log {
			address: H160::repeat_byte(0x01),
			topics: vec![H256::repeat_byte(0x02)],
			data: fc_rpc_core::types::Bytes(vec![0x03]),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		let config = LogsJournalConfig {
			max_entries: 8,
			max_total_logs: usize::MAX,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: 8,
			max_bytes_per_entry: usize::MAX,
		};
		let mut state = LogsJournalState::with_config(&config);
		let complete = state.push(true, vec![sample_log]);
		assert!(complete.complete);
		let gap = state.push(false, Vec::new());
		assert!(!gap.complete);
		assert!(gap.logs.is_empty());
		assert_eq!(gap.seq, complete.seq.saturating_add(1));
	}

	/// Mirrors the journal worker reconnect branch: push a gap marker only when `entries.back()`
	/// is complete; if the tail is already incomplete, return `None` (no stacked gap entries).
	#[test]
	fn gap_marker_not_pushed_again_when_tail_already_incomplete() {
		let sample_log = Log {
			address: H160::repeat_byte(0x01),
			topics: vec![H256::repeat_byte(0x02)],
			data: fc_rpc_core::types::Bytes(vec![0x03]),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		let config = LogsJournalConfig {
			max_entries: 8,
			max_total_logs: usize::MAX,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: 8,
			max_bytes_per_entry: usize::MAX,
		};
		let mut state = LogsJournalState::with_config(&config);

		state.push(true, vec![sample_log]);

		let maybe_first = match state.entries.back() {
			Some(last) if last.entry.complete => Some(state.push(false, Vec::new())),
			_ => None,
		};
		assert!(maybe_first.is_some());
		assert_eq!(state.entries.len(), 2);
		assert!(!state.entries.back().expect("tail").entry.complete);

		let maybe_second = match state.entries.back() {
			Some(last) if last.entry.complete => Some(state.push(false, Vec::new())),
			_ => None,
		};
		assert!(maybe_second.is_none());
		assert_eq!(state.entries.len(), 2);
	}

	/// Documents the dependency on Tokio `broadcast` lag semantics used by `eth_subscribe("logs")`
	/// (same channel capacity as `LogsJournal::with_config`); this is not a unit test of journal logic.
	#[test]
	fn journal_broadcast_matches_channel_capacity_for_ws_lag() {
		let cap = 4usize;
		let (tx, mut rx) = broadcast::channel(broadcast_capacity(cap));
		let dummy = Arc::new(LogsJournalEntry {
			seq: 0,
			complete: true,
			logs: Vec::new(),
		});
		futures::executor::block_on(async move {
			for _ in 0..cap {
				let _ = tx.send(dummy.clone());
				assert!(rx.recv().await.is_ok());
			}
			for _ in 0..cap.saturating_add(2) {
				let _ = tx.send(dummy.clone());
			}
			let next = rx.recv().await;
			assert!(
				matches!(next, Err(RecvError::Lagged(_))),
				"expected Lagged when subscriber falls behind the journal broadcast buffer: {next:?}"
			);
		});
	}

	#[test]
	fn normalized_preserves_explicit_limits() {
		let config = LogsJournalConfig {
			max_entries: 7,
			max_total_logs: 33,
			max_total_bytes: 123_456,
			max_blocks_per_entry: 11,
			max_logs_per_entry: 22,
			max_bytes_per_entry: 333,
		}
		.normalized();

		assert_eq!(config.max_entries, 7);
		assert_eq!(config.max_total_logs, 33);
		assert_eq!(config.max_total_bytes, 123_456);
		assert_eq!(config.max_blocks_per_entry, 11);
		assert_eq!(config.max_logs_per_entry, 22);
		assert_eq!(config.max_bytes_per_entry, 333);
	}

	#[test]
	fn from_max_total_bytes_clamps_entry_budget_to_total_budget() {
		let total_budget = 1024usize;
		let config = LogsJournalConfig::from_max_total_bytes(total_budget);

		assert_eq!(config.max_total_bytes, total_budget);
		assert_eq!(config.max_bytes_per_entry, total_budget);
		assert_eq!(config.max_entries, 1);
	}

	#[test]
	fn normalized_fills_missing_limits_from_explicit_entries() {
		let config = LogsJournalConfig {
			max_entries: 3,
			max_total_logs: 0,
			max_total_bytes: 0,
			max_blocks_per_entry: 0,
			max_logs_per_entry: 0,
			max_bytes_per_entry: 0,
		}
		.normalized();

		assert_eq!(config.max_entries, 3);
		assert_eq!(
			config.max_total_logs,
			3usize.saturating_mul(DEFAULT_LOGS_JOURNAL_MAX_LOGS_PER_ENTRY)
		);
		assert_eq!(
			config.max_total_bytes,
			3usize.saturating_mul(DEFAULT_LOGS_JOURNAL_MAX_BYTES_PER_ENTRY)
		);
		assert_eq!(
			config.max_blocks_per_entry,
			DEFAULT_LOGS_JOURNAL_MAX_BLOCKS_PER_ENTRY
		);
		assert_eq!(
			config.max_logs_per_entry,
			DEFAULT_LOGS_JOURNAL_MAX_LOGS_PER_ENTRY
		);
		assert_eq!(
			config.max_bytes_per_entry,
			DEFAULT_LOGS_JOURNAL_MAX_BYTES_PER_ENTRY
		);
	}

	#[test]
	fn broadcast_capacity_is_clamped() {
		assert_eq!(broadcast_capacity(0), 1);
		assert_eq!(broadcast_capacity(8), 8);
		assert_eq!(
			broadcast_capacity(MAX_BROADCAST_CAPACITY.saturating_add(1)),
			MAX_BROADCAST_CAPACITY
		);
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
		let (complete, logs) =
			build_journal_payload(&storage, notification, &LogsJournalConfig::default());

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
		let (complete, logs) =
			build_journal_payload(&storage, notification, &LogsJournalConfig::default());

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
	fn build_payload_with_reorg_visits_multiple_enacted_then_new_best() {
		let retracted = H256::repeat_byte(0x10);
		let enacted_a = H256::repeat_byte(0x20);
		let enacted_b = H256::repeat_byte(0x21);
		let new_best = H256::repeat_byte(0x30);

		let mut storage = MockStorageOverride::default();
		for (seed, hash) in [retracted, enacted_a, enacted_b, new_best]
			.iter()
			.enumerate()
		{
			storage
				.blocks
				.insert(*hash, make_ethereum_block(seed as u64));
			storage.statuses.insert(
				*hash,
				vec![make_status(H256::repeat_byte(0xA0 + seed as u8), 0)],
			);
		}

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash: new_best,
			reorg_info: Some(Arc::new(fc_mapping_sync::ReorgInfo::<OpaqueBlock> {
				common_ancestor: H256::repeat_byte(0x01),
				retracted: vec![retracted],
				enacted: vec![enacted_a, enacted_b],
				new_best,
			})),
		};
		let (complete, logs) =
			build_journal_payload(&storage, notification, &LogsJournalConfig::default());

		assert!(complete);
		assert_eq!(logs.len(), 4);
		assert!(logs[0].removed);
		assert!(!logs[1].removed);
		assert!(!logs[2].removed);
		assert!(!logs[3].removed);
		assert_eq!(logs[0].topics, vec![H256::repeat_byte(0xA0)]);
		assert_eq!(logs[1].topics, vec![H256::repeat_byte(0xA1)]);
		assert_eq!(logs[2].topics, vec![H256::repeat_byte(0xA2)]);
		assert_eq!(logs[3].topics, vec![H256::repeat_byte(0xA3)]);
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
		let (complete, logs) =
			build_journal_payload(&storage, notification, &LogsJournalConfig::default());

		assert!(!complete);
		assert!(logs.is_empty());
	}

	#[test]
	fn state_prunes_by_total_retained_bytes() {
		let sample_log = Log {
			address: H160::repeat_byte(0x01),
			topics: vec![H256::repeat_byte(0x02)],
			data: fc_rpc_core::types::Bytes(vec![0x03; 128]),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		let retained_bytes = retained_entry_bytes(&vec![sample_log.clone()]);
		// Keep the config explicit here so this test isolates the `max_total_bytes` eviction path.
		let config = LogsJournalConfig {
			max_entries: 8,
			max_total_logs: usize::MAX,
			max_total_bytes: retained_bytes.saturating_add(1),
			max_blocks_per_entry: 8,
			max_logs_per_entry: 8,
			max_bytes_per_entry: usize::MAX,
		};
		let mut state = LogsJournalState::with_config(&config);
		let first = vec![sample_log];
		let second = first.clone();

		state.push(true, first);
		state.push(true, second);

		assert_eq!(state.entries.len(), 1);
		assert_eq!(state.earliest_available(), 1);
		assert_eq!(state.total_bytes, retained_bytes);
	}

	#[test]
	fn state_prunes_by_max_entries() {
		let sample_log = Log {
			address: H160::repeat_byte(0x01),
			topics: vec![H256::repeat_byte(0x02)],
			data: fc_rpc_core::types::Bytes(vec![0x03; 4]),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		let retained = retained_entry_bytes(&vec![sample_log.clone()]);
		let config = LogsJournalConfig {
			max_entries: 2,
			max_total_logs: usize::MAX,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: 8,
			max_bytes_per_entry: usize::MAX,
		};
		let mut state = LogsJournalState::with_config(&config);

		state.push(true, vec![sample_log.clone()]);
		state.push(true, vec![sample_log.clone()]);
		assert_eq!(state.entries.len(), 2);
		assert_eq!(state.total_logs, 2);
		assert_eq!(state.total_bytes, retained.saturating_mul(2));

		state.push(true, vec![sample_log]);

		assert_eq!(state.entries.len(), 2);
		assert_eq!(state.earliest_available(), 1);
		assert_eq!(state.total_logs, 2);
		assert_eq!(state.total_bytes, retained.saturating_mul(2));
	}

	#[test]
	fn build_payload_returns_incomplete_when_reorg_exceeds_block_cap() {
		let mut storage = MockStorageOverride::default();
		let new_best = H256::repeat_byte(0x30);
		let retracted = (0..65usize)
			.map(|i| H256::from_low_u64_be(0x100 + i as u64))
			.collect::<Vec<_>>();
		let enacted = (0..63usize)
			.map(|i| H256::from_low_u64_be(0x200 + i as u64))
			.collect::<Vec<_>>();

		for (seed, hash) in retracted
			.iter()
			.chain(enacted.iter())
			.chain(std::iter::once(&new_best))
			.enumerate()
		{
			storage
				.blocks
				.insert(*hash, make_ethereum_block(seed as u64));
			storage.statuses.insert(
				*hash,
				vec![make_status(H256::from_low_u64_be(0xA0 + seed as u64), 0)],
			);
		}

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash: new_best,
			reorg_info: Some(Arc::new(fc_mapping_sync::ReorgInfo::<OpaqueBlock> {
				common_ancestor: H256::repeat_byte(0x01),
				retracted,
				enacted,
				new_best,
			})),
		};
		let config = LogsJournalConfig::default();

		let (complete, logs) = build_journal_payload(&storage, notification, &config);
		assert!(!complete);
		assert!(logs.is_empty());
	}

	#[test]
	fn build_payload_returns_incomplete_when_log_count_cap_is_exceeded() {
		let hash = H256::repeat_byte(0xAA);
		let mut storage = MockStorageOverride::default();
		storage.blocks.insert(hash, make_ethereum_block(10));
		storage.statuses.insert(
			hash,
			vec![transaction_status_with_logs(
				0xA0,
				0,
				DEFAULT_LOGS_JOURNAL_MAX_LOGS_PER_ENTRY.saturating_add(1),
				2,
			)],
		);

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash,
			reorg_info: None,
		};
		let config = LogsJournalConfig::default();

		let (complete, logs) = build_journal_payload(&storage, notification, &config);
		assert!(!complete);
		assert!(logs.is_empty());
	}

	#[test]
	fn build_payload_returns_incomplete_when_byte_cap_is_exceeded() {
		let hash = H256::repeat_byte(0xAA);
		let mut storage = MockStorageOverride::default();
		storage.blocks.insert(hash, make_ethereum_block(10));
		storage.statuses.insert(
			hash,
			vec![transaction_status_with_logs(
				0xA0,
				0,
				1,
				DEFAULT_LOGS_JOURNAL_MAX_BYTES_PER_ENTRY.saturating_add(1),
			)],
		);

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash,
			reorg_info: None,
		};
		let config = LogsJournalConfig::default();

		let (complete, logs) = build_journal_payload(&storage, notification, &config);
		assert!(!complete);
		assert!(logs.is_empty());
	}

	#[test]
	fn build_payload_rejects_single_log_when_retained_size_exceeds_cap_after_push() {
		let hash = H256::repeat_byte(0xAA);
		let mut storage = MockStorageOverride::default();
		storage.blocks.insert(hash, make_ethereum_block(10));
		storage
			.statuses
			.insert(hash, vec![transaction_status_with_logs(0xA0, 0, 1, 2)]);

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash,
			reorg_info: None,
		};
		let loose_config = LogsJournalConfig {
			max_entries: 1,
			max_total_logs: usize::MAX,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: 8,
			max_bytes_per_entry: usize::MAX,
		};
		let (complete_loose, logs_loose) =
			build_journal_payload(&storage, notification.clone(), &loose_config);
		assert!(complete_loose);
		assert_eq!(logs_loose.len(), 1);

		let retained_bytes = retained_entry_bytes(&logs_loose);
		let tight_config = LogsJournalConfig {
			max_entries: 1,
			max_total_logs: usize::MAX,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: 8,
			max_bytes_per_entry: retained_bytes.saturating_sub(1),
		};
		let (complete_tight, logs_tight) =
			build_journal_payload(&storage, notification, &tight_config);

		assert!(!complete_tight);
		assert!(logs_tight.is_empty());
	}

	/// Cumulative path: many small logs that fit under the cap when taken alone, but retained
	/// accounting grows across `append_block_logs` pushes (`dynamic_bytes`, `Vec` slab via
	/// `capacity`, etc.) until `max_bytes_per_entry` is exceeded — unlike a single oversized
	/// `data` buffer in `build_payload_returns_incomplete_when_byte_cap_is_exceeded`.
	#[test]
	fn build_payload_returns_incomplete_when_cumulative_bytes_exceed_entry_cap() {
		let hash = H256::repeat_byte(0xAA);
		let log_count = 64usize;
		let per_log_data_len = 1024usize;
		let mut storage = MockStorageOverride::default();
		storage.blocks.insert(hash, make_ethereum_block(10));
		storage.statuses.insert(
			hash,
			vec![transaction_status_with_logs(
				0xA0,
				0,
				log_count,
				per_log_data_len,
			)],
		);

		let notification = EthereumBlockNotification::<OpaqueBlock> {
			is_new_best: true,
			hash,
			reorg_info: None,
		};
		let loose_config = LogsJournalConfig {
			max_entries: 8,
			max_total_logs: usize::MAX,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: log_count.saturating_add(1),
			max_bytes_per_entry: usize::MAX,
		};
		let (complete_loose, logs_loose) =
			build_journal_payload(&storage, notification.clone(), &loose_config);
		assert!(
			complete_loose,
			"same block should build completely when per-entry byte cap is loose"
		);
		assert_eq!(
			logs_loose.len(),
			log_count,
			"positive control: expect every mock log when not byte-limited"
		);

		// Tight cap: typically exceeded partway through the block as retained size grows, not
		// only after the last log (Vec `capacity` and dynamic totals accumulate per push).
		let tight_config = LogsJournalConfig {
			max_entries: 8,
			max_total_logs: usize::MAX,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: log_count.saturating_add(1),
			max_bytes_per_entry: 8 * 1024,
		};
		let (complete_tight, logs_tight) =
			build_journal_payload(&storage, notification, &tight_config);
		assert!(
			!complete_tight,
			"expected incomplete once cumulative retained size exceeds per-entry cap"
		);
		assert!(
			logs_tight.is_empty(),
			"append_block_logs fails closed with empty logs on cap breach"
		);
	}

	/// Eviction is an OR across `max_entries | max_total_logs | max_total_bytes`. This pins the
	/// `max_total_logs` branch without interference from the other two.
	#[test]
	fn state_prunes_by_total_retained_logs() {
		let sample_log = Log {
			address: H160::repeat_byte(0x01),
			topics: vec![H256::repeat_byte(0x02)],
			data: fc_rpc_core::types::Bytes(vec![0x03; 8]),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		let config = LogsJournalConfig {
			max_entries: 8,
			max_total_logs: 2,
			max_total_bytes: usize::MAX,
			max_blocks_per_entry: 8,
			max_logs_per_entry: 8,
			max_bytes_per_entry: usize::MAX,
		};
		let mut state = LogsJournalState::with_config(&config);

		state.push(true, vec![sample_log.clone()]);
		state.push(true, vec![sample_log.clone()]);
		assert_eq!(state.entries.len(), 2);
		assert_eq!(state.earliest_available(), 0);

		state.push(true, vec![sample_log]);

		assert_eq!(state.entries.len(), 2);
		assert_eq!(state.earliest_available(), 1);
		assert_eq!(state.total_logs, 2);
	}

	/// Regression for `visit_block_logs_with_removed` (used by `append_block_logs` for caps): a
	/// visitor `Break` must stop walking the block. This is filter-layer behavior, not the full
	/// journal pipeline.
	#[test]
	fn visit_block_logs_honors_control_flow_break() {
		let block = make_ethereum_block(1);
		let statuses = vec![transaction_status_with_logs(0xA0, 0, 5, 2)];
		let mut seen = 0usize;
		let outcome =
			visit_block_logs_with_removed(&Filter::default(), block, statuses, false, |_log| {
				seen += 1;
				if seen == 3 {
					std::ops::ControlFlow::Break(())
				} else {
					std::ops::ControlFlow::Continue(())
				}
			});
		assert!(matches!(outcome, std::ops::ControlFlow::Break(())));
		assert_eq!(seen, 3);
	}
}
