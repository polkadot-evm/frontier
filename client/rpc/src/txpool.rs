// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{marker::PhantomData, sync::Arc};

use ethereum::TransactionV2 as EthereumTransaction;
use ethereum_types::{H160, H256, U256};
use jsonrpsee::core::RpcResult;
use serde::Serialize;
// substrate
use sc_transaction_pool::{ChainApi, Pool};
use sc_transaction_pool_api::InPoolTransaction;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::{
	types::{BuildFrom, Summary, Transaction, TransactionMap, TxPoolResult},
	TxPoolApiServer,
};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{internal_err, public_key};

struct TxPoolTransactions {
	ready: Vec<EthereumTransaction>,
	future: Vec<EthereumTransaction>,
}

pub struct TxPool<B, C, A: ChainApi> {
	client: Arc<C>,
	graph: Arc<Pool<A>>,
	_marker: PhantomData<B>,
}

impl<B, C, A: ChainApi> Clone for TxPool<B, C, A> {
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
			graph: self.graph.clone(),
			_marker: PhantomData,
		}
	}
}

impl<B, C, A> TxPool<B, C, A>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + 'static,
	A: ChainApi<Block = B> + 'static,
{
	fn map_build<T>(&self) -> RpcResult<TxPoolResult<TransactionMap<T>>>
	where
		T: BuildFrom + Serialize,
	{
		let txns = self.collect_txpool_transactions()?;
		let pending = Self::build_txn_map::<'_, T>(txns.ready.iter());
		let queued = Self::build_txn_map::<'_, T>(txns.future.iter());
		Ok(TxPoolResult { pending, queued })
	}

	fn build_txn_map<'a, T>(
		txns: impl Iterator<Item = &'a EthereumTransaction>,
	) -> TransactionMap<T>
	where
		T: BuildFrom + Serialize,
	{
		let mut result = TransactionMap::<T>::new();
		for txn in txns {
			let nonce = match txn {
				EthereumTransaction::Legacy(t) => t.nonce,
				EthereumTransaction::EIP2930(t) => t.nonce,
				EthereumTransaction::EIP1559(t) => t.nonce,
			};
			let from = match public_key(txn) {
				Ok(pk) => H160::from(H256::from(keccak_256(&pk))),
				Err(_) => H160::default(),
			};
			result
				.entry(from)
				.or_default()
				.insert(nonce, T::build_from(from, txn));
		}
		result
	}

	/// Collect the extrinsics currently in the ready and future queues.
	fn collect_txpool_transactions(&self) -> RpcResult<TxPoolTransactions> {
		// Collect extrinsics in the ready validated pool.
		let ready_extrinsics = self
			.graph
			.validated_pool()
			.ready()
			.map(|in_pool_tx| in_pool_tx.data().clone())
			.collect();

		// Collect extrinsics in the future validated pool.
		let future_extrinsics = self
			.graph
			.validated_pool()
			.futures()
			.iter()
			.map(|(_, extrinsic)| extrinsic.clone())
			.collect();

		// Use the runtime to match the (here) opaque extrinsics against ethereum transactions.
		let best_block = self.client.info().best_hash;
		let api = self.client.runtime_api();
		let ready = api
			.extrinsic_filter(best_block, ready_extrinsics)
			.map_err(|err| internal_err(format!("fetch ready transactions failed: {err}")))?;
		let future = api
			.extrinsic_filter(best_block, future_extrinsics)
			.map_err(|err| internal_err(format!("fetch future transactions failed: {err}")))?;

		Ok(TxPoolTransactions { ready, future })
	}
}

impl<B, C, A: ChainApi> TxPool<B, C, A> {
	pub fn new(client: Arc<C>, graph: Arc<Pool<A>>) -> Self {
		Self {
			client,
			graph,
			_marker: PhantomData,
		}
	}
}

impl<B, C, A> TxPoolApiServer for TxPool<B, C, A>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + 'static,
	A: ChainApi<Block = B> + 'static,
{
	fn content(&self) -> RpcResult<TxPoolResult<TransactionMap<Transaction>>> {
		self.map_build::<Transaction>()
	}

	fn inspect(&self) -> RpcResult<TxPoolResult<TransactionMap<Summary>>> {
		self.map_build::<Summary>()
	}

	fn status(&self) -> RpcResult<TxPoolResult<U256>> {
		let status = self.graph.validated_pool().status();
		Ok(TxPoolResult {
			pending: U256::from(status.ready),
			queued: U256::from(status.future),
		})
	}
}
