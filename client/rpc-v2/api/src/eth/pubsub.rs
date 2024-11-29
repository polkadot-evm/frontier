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

use jsonrpsee::{core::SubscriptionResult, proc_macros::rpc};

use crate::types::pubsub::{PubSubKind, PubSubParams, PubSubResult};

/// (Non-standard) Ethereum pubsub interface.
///
/// It's not part of the standard interface for Ethereum clients, but this interface is very useful
/// and almost all Ethereum clients implement this interface. So we also provide the interface
/// compatible with geth.
#[rpc(client, server, namespace = "eth")]
#[async_trait]
pub trait EthPubSubApi {
	/// Create an ethereum subscription for the given params
	#[subscription(
		name = "subscribe" => "subscription",
		unsubscribe = "unsubscribe",
		item = PubSubResult
	)]
	async fn sub(&self, kind: PubSubKind, params: Option<PubSubParams>) -> SubscriptionResult;
}
