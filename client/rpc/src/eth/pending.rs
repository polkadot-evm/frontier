// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
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

use std::{marker::PhantomData, sync::Arc};

// Substrate
use sc_client_api::{
	backend::{AuxStore, Backend, StorageProvider},
	UsageProvider,
};
use sc_consensus_manual_seal::Error;
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::InPoolTransaction;
use sp_api::{ApiExt, ApiRef, Core, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::{ApplyExtrinsicFailed, HeaderBackend};
use sp_inherents::{CreateInherentDataProviders, InherentData, InherentDataProvider};
use sp_runtime::{
	generic::{Digest, DigestItem},
	traits::{Block as BlockT, Header as HeaderT, One},
	TransactionOutcome,
};
use sp_timestamp::TimestampInherentData;

use crate::eth::{Eth, EthConfig};

const LOG_TARGET: &str = "eth-pending";

impl<B, C, P, CT, BE, A, CIDP, EC> Eth<B, C, P, CT, BE, A, CIDP, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B>,
	A: ChainApi<Block = B>,
	CIDP: CreateInherentDataProviders<B, ()> + Send + 'static,
	EC: EthConfig<B, C>,
{
	/// Creates a pending runtime API.
	pub(crate) async fn pending_runtime_api(&self) -> Result<(B::Hash, ApiRef<C::Api>), Error> {
		let api = self.client.runtime_api();

		let info = self.client.info();
		let (best_number, best_hash) = (info.best_number, info.best_hash);

		let inherent_data_provider = self
			.pending_create_inherent_data_providers
			.create_inherent_data_providers(best_hash, ())
			.await?;
		let inherent_data = inherent_data_provider.create_inherent_data().await?;

		let digest = if let Some(digest_provider) = &self.pending_consensus_data_provider {
			if let Some(header) = self.client.header(best_hash)? {
				digest_provider.create_digest(&header, &inherent_data)?
			} else {
				Default::default()
			}
		} else {
			Default::default()
		};

		log::info!(target: LOG_TARGET, "Pending runtime API: header digest = {digest:?}");

		let pending_header = <<B as BlockT>::Header as HeaderT>::new(
			best_number + One::one(),
			Default::default(),
			Default::default(),
			best_hash,
			digest,
		);

		// Initialize the pending block header
		api.initialize_block(best_hash, &pending_header)?;

		// Apply inherents to the pending block.
		let inherents = api.execute_in_transaction(move |api| {
			// `create_inherents` should not change any state, to ensure this we always rollback
			// the transaction.
			TransactionOutcome::Rollback(api.inherent_extrinsics(best_hash, inherent_data))
		})?;
		log::debug!(target: LOG_TARGET, "Pending runtime API: inherent len = {}", inherents.len());
		// Apply the inherents to the best block's state.
		for ext in inherents {
			let _ = api.execute_in_transaction(|api| match api.apply_extrinsic(best_hash, ext) {
				Ok(Ok(_)) => TransactionOutcome::Commit(Ok(())),
				Ok(Err(tx_validity)) => TransactionOutcome::Rollback(Err(
					ApplyExtrinsicFailed::Validity(tx_validity).into(),
				)),
				Err(err) => TransactionOutcome::Rollback(Err(Error::from(err))),
			});
		}

		// Get all extrinsics from the ready queue.
		let extrinsics: Vec<<B as BlockT>::Extrinsic> = self
			.graph
			.validated_pool()
			.ready()
			.map(|in_pool_tx| in_pool_tx.data().clone())
			.collect::<Vec<<B as BlockT>::Extrinsic>>();
		log::debug!(target: LOG_TARGET, "Pending runtime API: extrinsic len = {}", extrinsics.len());
		// Apply the extrinsics from the ready queue to the pending block's state.
		for ext in extrinsics {
			let _ = api.execute_in_transaction(|api| match api.apply_extrinsic(best_hash, ext) {
				Ok(Ok(_)) => TransactionOutcome::Commit(Ok(())),
				Ok(Err(tx_validity)) => TransactionOutcome::Rollback(Err(
					ApplyExtrinsicFailed::Validity(tx_validity).into(),
				)),
				Err(err) => TransactionOutcome::Rollback(Err(Error::from(err))),
			});
		}

		Ok((best_hash, api))
	}
}
