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

use fp_rpc::EthereumRuntimeRPCApi;
use futures::{
	prelude::*,
	task::{Context, Poll},
};
use futures_timer::Delay;
use log::debug;
use sc_client_api::{BlockOf, ImportNotifications};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{pin::Pin, sync::Arc, time::Duration};

#[derive(PartialEq, Copy, Clone)]
pub enum SyncStrategy {
	Normal,
	Parachain,
}

pub struct MappingSyncWorker<Block: BlockT, C, B> {
	import_notifications: ImportNotifications<Block>,
	timeout: Duration,
	inner_delay: Option<Delay>,

	client: Arc<C>,
	substrate_backend: Arc<B>,
	frontier_backend: Arc<fc_db::Backend<Block>>,

	have_next: bool,
	retry_times: usize,
	sync_from: <Block::Header as HeaderT>::Number,
	strategy: SyncStrategy,
}

impl<Block: BlockT, C, B> Unpin for MappingSyncWorker<Block, C, B> {}

impl<Block: BlockT, C, B> MappingSyncWorker<Block, C, B> {
	pub fn new(
		import_notifications: ImportNotifications<Block>,
		timeout: Duration,
		client: Arc<C>,
		substrate_backend: Arc<B>,
		frontier_backend: Arc<fc_db::Backend<Block>>,
		retry_times: usize,
		sync_from: <Block::Header as HeaderT>::Number,
		strategy: SyncStrategy,
	) -> Self {
		Self {
			import_notifications,
			timeout,
			inner_delay: None,

			client,
			substrate_backend,
			frontier_backend,

			have_next: true,
			retry_times,
			sync_from,
			strategy,
		}
	}
}

impl<Block: BlockT, C, B> Stream for MappingSyncWorker<Block, C, B>
where
	C: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + BlockOf,
	C::Api: EthereumRuntimeRPCApi<Block>,
	B: sc_client_api::Backend<Block>,
{
	type Item = ();

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<()>> {
		let mut fire = false;

		loop {
			match Stream::poll_next(Pin::new(&mut self.import_notifications), cx) {
				Poll::Pending => break,
				Poll::Ready(Some(_)) => {
					fire = true;
				}
				Poll::Ready(None) => return Poll::Ready(None),
			}
		}

		let timeout = self.timeout.clone();
		let inner_delay = self.inner_delay.get_or_insert_with(|| Delay::new(timeout));

		match Future::poll(Pin::new(inner_delay), cx) {
			Poll::Pending => (),
			Poll::Ready(()) => {
				fire = true;
			}
		}

		if self.have_next {
			fire = true;
		}

		if fire {
			self.inner_delay = None;

			match crate::sync_blocks(
				self.client.as_ref(),
				self.substrate_backend.blockchain(),
				self.frontier_backend.as_ref(),
				self.retry_times,
				self.sync_from,
				self.strategy,
			) {
				Ok(have_next) => {
					self.have_next = have_next;
					Poll::Ready(Some(()))
				}
				Err(e) => {
					self.have_next = false;
					debug!(target: "mapping-sync", "Syncing failed with error {:?}, retrying.", e);
					Poll::Ready(Some(()))
				}
			}
		} else {
			Poll::Pending
		}
	}
}
