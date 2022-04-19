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

mod eth;
mod eth_pubsub;
mod net;
mod overrides;
mod signer;
mod web3;

pub use self::{
	eth::{EthApi, EthBlockDataCache, EthFilterApi, EthTask},
	eth_pubsub::{EthPubSubApi, HexEncodedIdProvider},
	net::NetApi,
	overrides::{
		OverrideHandle, RuntimeApiStorageOverride, SchemaV1Override, SchemaV2Override,
		SchemaV3Override, StorageOverride,
	},
	signer::{EthDevSigner, EthSigner},
	web3::Web3Api,
};

pub use ethereum::TransactionV2 as EthereumTransaction;
pub use fc_rpc_core::{
	EthApiServer, EthFilterApiServer, EthPubSubApiServer, NetApiServer, Web3ApiServer,
};

pub mod frontier_backend_client {
	use super::internal_err;

	use codec::Decode;
	use ethereum_types::H256;
	use jsonrpc_core::Result as RpcResult;

	use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
	use sp_blockchain::HeaderBackend;
	use sp_runtime::{
		generic::BlockId,
		traits::{BlakeTwo256, Block as BlockT, Header as HeaderT, UniqueSaturatedInto, Zero},
	};
	use sp_storage::StorageKey;

	use fc_rpc_core::types::BlockNumber;
	use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};

	pub fn native_block_id<B: BlockT, C>(
		client: &C,
		backend: &fc_db::Backend<B>,
		number: Option<BlockNumber>,
	) -> RpcResult<Option<BlockId<B>>>
	where
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: HeaderBackend<B> + Send + Sync + 'static,
	{
		Ok(match number.unwrap_or(BlockNumber::Latest) {
			BlockNumber::Hash { hash, .. } => load_hash::<B>(backend, hash).unwrap_or(None),
			BlockNumber::Num(number) => Some(BlockId::Number(number.unique_saturated_into())),
			BlockNumber::Latest => Some(BlockId::Hash(client.info().best_hash)),
			BlockNumber::Earliest => Some(BlockId::Number(Zero::zero())),
			BlockNumber::Pending => None,
		})
	}

	pub fn load_hash<B: BlockT>(
		backend: &fc_db::Backend<B>,
		hash: H256,
	) -> RpcResult<Option<BlockId<B>>>
	where
		B: BlockT<Hash = H256> + Send + Sync + 'static,
	{
		let substrate_hash = backend
			.mapping()
			.block_hash(&hash)
			.map_err(|err| internal_err(format!("fetch aux store failed: {:?}", err)))?;

		if let Some(substrate_hash) = substrate_hash {
			return Ok(Some(BlockId::Hash(substrate_hash)));
		}
		Ok(None)
	}

	pub fn load_cached_schema<B: BlockT>(
		backend: &fc_db::Backend<B>,
	) -> RpcResult<Option<Vec<(EthereumStorageSchema, H256)>>>
	where
		B: BlockT<Hash = H256> + Send + Sync + 'static,
	{
		let cache = backend
			.meta()
			.ethereum_schema()
			.map_err(|err| internal_err(format!("fetch backend failed: {:?}", err)))?;
		Ok(cache)
	}

	pub fn write_cached_schema<B: BlockT>(
		backend: &fc_db::Backend<B>,
		new_cache: Vec<(EthereumStorageSchema, H256)>,
	) -> RpcResult<()>
	where
		B: BlockT<Hash = H256> + Send + Sync + 'static,
	{
		backend
			.meta()
			.write_ethereum_schema(new_cache)
			.map_err(|err| internal_err(format!("write backend failed: {:?}", err)))?;
		Ok(())
	}

	pub fn onchain_storage_schema<B: BlockT, C, BE>(
		client: &C,
		at: BlockId<B>,
	) -> EthereumStorageSchema
	where
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: StorageProvider<B, BE> + Send + Sync + 'static,
		BE: Backend<B> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		match client.storage(&at, &StorageKey(PALLET_ETHEREUM_SCHEMA.to_vec())) {
			Ok(Some(bytes)) => Decode::decode(&mut &bytes.0[..])
				.ok()
				.unwrap_or(EthereumStorageSchema::Undefined),
			_ => EthereumStorageSchema::Undefined,
		}
	}

	pub fn is_canon<B: BlockT, C>(client: &C, target_hash: H256) -> bool
	where
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: HeaderBackend<B> + Send + Sync + 'static,
	{
		if let Ok(Some(number)) = client.number(target_hash) {
			if let Ok(Some(header)) = client.header(BlockId::Number(number)) {
				return header.hash() == target_hash;
			}
		}
		false
	}

	pub fn load_transactions<B: BlockT, C>(
		client: &C,
		backend: &fc_db::Backend<B>,
		transaction_hash: H256,
		only_canonical: bool,
	) -> RpcResult<Option<(H256, u32)>>
	where
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: HeaderBackend<B> + Send + Sync + 'static,
	{
		let transaction_metadata = backend
			.mapping()
			.transaction_metadata(&transaction_hash)
			.map_err(|err| internal_err(format!("fetch aux store failed: {:?}", err)))?;

		transaction_metadata
			.iter()
			.find(|meta| is_canon::<B, C>(client, meta.block_hash))
			.map_or_else(
				|| {
					if !only_canonical && transaction_metadata.len() > 0 {
						Ok(Some((
							transaction_metadata[0].ethereum_block_hash,
							transaction_metadata[0].ethereum_index,
						)))
					} else {
						Ok(None)
					}
				},
				|meta| Ok(Some((meta.ethereum_block_hash, meta.ethereum_index))),
			)
	}
}

pub fn internal_err<T: ToString>(message: T) -> jsonrpc_core::Error {
	jsonrpc_core::Error {
		code: jsonrpc_core::ErrorCode::InternalError,
		message: message.to_string(),
		data: None,
	}
}

pub fn public_key(transaction: &EthereumTransaction) -> Result<[u8; 64], sp_io::EcdsaVerifyError> {
	let mut sig = [0u8; 65];
	let mut msg = [0u8; 32];
	match transaction {
		EthereumTransaction::Legacy(t) => {
			sig[0..32].copy_from_slice(&t.signature.r()[..]);
			sig[32..64].copy_from_slice(&t.signature.s()[..]);
			sig[64] = t.signature.standard_v();
			msg.copy_from_slice(&ethereum::LegacyTransactionMessage::from(t.clone()).hash()[..]);
		}
		EthereumTransaction::EIP2930(t) => {
			sig[0..32].copy_from_slice(&t.r[..]);
			sig[32..64].copy_from_slice(&t.s[..]);
			sig[64] = t.odd_y_parity as u8;
			msg.copy_from_slice(&ethereum::EIP2930TransactionMessage::from(t.clone()).hash()[..]);
		}
		EthereumTransaction::EIP1559(t) => {
			sig[0..32].copy_from_slice(&t.r[..]);
			sig[32..64].copy_from_slice(&t.s[..]);
			sig[64] = t.odd_y_parity as u8;
			msg.copy_from_slice(&ethereum::EIP1559TransactionMessage::from(t.clone()).hash()[..]);
		}
	}
	sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg)
}
