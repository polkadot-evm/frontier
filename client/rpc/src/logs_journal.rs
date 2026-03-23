// This file is part of Frontier.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::{
	collections::VecDeque,
	sync::{Arc, Mutex},
};

use futures::StreamExt as _;
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
		executor.spawn("frontier-rpc-logs-journal", Some("rpc"), async move {
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
		});

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

fn build_journal_payload<B: BlockT>(
	storage_override: &dyn StorageOverride<B>,
	notification: EthereumBlockNotification<B>,
) -> (bool, Vec<Log>) {
	let mut logs = Vec::new();
	let empty_filter = Filter::default();

	if let Some(reorg_info) = notification.reorg_info {
		for hash in reorg_info.retracted {
			if !append_block_logs(storage_override, &empty_filter, hash, true, &mut logs) {
				return (false, Vec::new());
			}
		}
		for hash in reorg_info
			.enacted
			.into_iter()
			.chain(std::iter::once(reorg_info.new_best))
		{
			if !append_block_logs(storage_override, &empty_filter, hash, false, &mut logs) {
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
}
