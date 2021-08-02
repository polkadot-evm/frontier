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

mod eth;
mod eth_pubsub;
mod overrides;

pub use eth::{
	EthApi, EthApiServer, EthFilterApi, EthFilterApiServer, EthTask, NetApi, NetApiServer, Web3Api,
	Web3ApiServer,
};
pub use eth_pubsub::{EthPubSubApi, EthPubSubApiServer, HexEncodedIdProvider};
pub use overrides::{OverrideHandle, RuntimeApiStorageOverride, SchemaV1Override, StorageOverride};

use ethereum::{
	Transaction as EthereumTransaction, TransactionMessage as EthereumTransactionMessage,
};
use ethereum_types::{H160, H256};
use evm::ExitError;
use jsonrpc_core::{Error, ErrorCode, Value};
use pallet_evm::ExitReason;
use rustc_hex::ToHex;
use sha3::{Digest, Keccak256};

pub mod frontier_backend_client {

	use super::internal_err;

	use fc_rpc_core::types::BlockNumber;
	use fp_storage::PALLET_ETHEREUM_SCHEMA;
	use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
	use sp_api::{BlockId, HeaderT};
	use sp_blockchain::HeaderBackend;
	use sp_runtime::traits::{BlakeTwo256, Block as BlockT, UniqueSaturatedInto, Zero};
	use sp_storage::StorageKey;

	use codec::Decode;
	use jsonrpc_core::Result as RpcResult;

	use ethereum_types::H256;
	use pallet_ethereum::EthereumStorageSchema;

	pub fn native_block_id<B: BlockT, C>(
		client: &C,
		backend: &fc_db::Backend<B>,
		number: Option<BlockNumber>,
	) -> RpcResult<Option<BlockId<B>>>
	where
		B: BlockT,
		C: HeaderBackend<B> + 'static,
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: Send + Sync + 'static,
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
		B: BlockT,
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
		B: BlockT,
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
		B: BlockT,
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
		B: BlockT,
		C: StorageProvider<B, BE>,
		BE: Backend<B> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: Send + Sync + 'static,
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
		B: BlockT,
		C: HeaderBackend<B> + 'static,
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: Send + Sync + 'static,
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
	) -> RpcResult<Option<(H256, u32)>>
	where
		B: BlockT,
		C: HeaderBackend<B> + 'static,
		B: BlockT<Hash = H256> + Send + Sync + 'static,
		C: Send + Sync + 'static,
	{
		let transaction_metadata = backend
			.mapping()
			.transaction_metadata(&transaction_hash)
			.map_err(|err| internal_err(format!("fetch aux store failed: {:?}", err)))?;

		if transaction_metadata.len() == 1 {
			Ok(Some((
				transaction_metadata[0].ethereum_block_hash,
				transaction_metadata[0].ethereum_index,
			)))
		} else if transaction_metadata.len() > 1 {
			transaction_metadata
				.iter()
				.find(|meta| is_canon::<B, C>(client, meta.block_hash))
				.map_or(
					Ok(Some((
						transaction_metadata[0].ethereum_block_hash,
						transaction_metadata[0].ethereum_index,
					))),
					|meta| Ok(Some((meta.ethereum_block_hash, meta.ethereum_index))),
				)
		} else {
			Ok(None)
		}
	}
}

pub fn internal_err<T: ToString>(message: T) -> Error {
	Error {
		code: ErrorCode::InternalError,
		message: message.to_string(),
		data: None,
	}
}

pub fn error_on_execution_failure(reason: &ExitReason, data: &[u8]) -> Result<(), Error> {
	match reason {
		ExitReason::Succeed(_) => Ok(()),
		ExitReason::Error(e) => {
			if *e == ExitError::OutOfGas {
				// `ServerError(0)` will be useful in estimate gas
				return Err(Error {
					code: ErrorCode::ServerError(0),
					message: format!("out of gas"),
					data: None,
				});
			}
			Err(Error {
				code: ErrorCode::InternalError,
				message: format!("evm error: {:?}", e),
				data: Some(Value::String("0x".to_string())),
			})
		}
		ExitReason::Revert(_) => {
			let mut message = "VM Exception while processing transaction: revert".to_string();
			// A minimum size of error function selector (4) + offset (32) + string length (32)
			// should contain a utf-8 encoded revert reason.
			if data.len() > 68 {
				let message_len = data[36..68].iter().sum::<u8>();
				let body: &[u8] = &data[68..68 + message_len as usize];
				if let Ok(reason) = std::str::from_utf8(body) {
					message = format!("{} {}", message, reason.to_string());
				}
			}
			Err(Error {
				code: ErrorCode::InternalError,
				message,
				data: Some(Value::String(data.to_hex())),
			})
		}
		ExitReason::Fatal(e) => Err(Error {
			code: ErrorCode::InternalError,
			message: format!("evm fatal: {:?}", e),
			data: Some(Value::String("0x".to_string())),
		}),
	}
}

pub fn public_key(transaction: &EthereumTransaction) -> Result<[u8; 64], sp_io::EcdsaVerifyError> {
	let mut sig = [0u8; 65];
	let mut msg = [0u8; 32];
	sig[0..32].copy_from_slice(&transaction.signature.r()[..]);
	sig[32..64].copy_from_slice(&transaction.signature.s()[..]);
	sig[64] = transaction.signature.standard_v();
	msg.copy_from_slice(&EthereumTransactionMessage::from(transaction.clone()).hash()[..]);

	sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg)
}

/// A generic Ethereum signer.
pub trait EthSigner: Send + Sync {
	/// Available accounts from this signer.
	fn accounts(&self) -> Vec<H160>;
	/// Sign a transaction message using the given account in message.
	fn sign(
		&self,
		message: ethereum::TransactionMessage,
		address: &H160,
	) -> Result<ethereum::Transaction, Error>;
}

pub struct EthDevSigner {
	keys: Vec<secp256k1::SecretKey>,
}

impl EthDevSigner {
	pub fn new() -> Self {
		Self {
			keys: vec![secp256k1::SecretKey::parse(&[
				0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
				0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
				0x11, 0x11, 0x11, 0x11,
			])
			.expect("Test key is valid; qed")],
		}
	}
}

impl EthSigner for EthDevSigner {
	fn accounts(&self) -> Vec<H160> {
		self.keys
			.iter()
			.map(|secret| {
				let public = secp256k1::PublicKey::from_secret_key(secret);
				let mut res = [0u8; 64];
				res.copy_from_slice(&public.serialize()[1..65]);

				H160::from(H256::from_slice(Keccak256::digest(&res).as_slice()))
			})
			.collect()
	}

	fn sign(
		&self,
		message: ethereum::TransactionMessage,
		address: &H160,
	) -> Result<ethereum::Transaction, Error> {
		let mut transaction = None;

		for secret in &self.keys {
			let key_address = {
				let public = secp256k1::PublicKey::from_secret_key(secret);
				let mut res = [0u8; 64];
				res.copy_from_slice(&public.serialize()[1..65]);
				H160::from(H256::from_slice(Keccak256::digest(&res).as_slice()))
			};

			if &key_address == address {
				let signing_message = secp256k1::Message::parse_slice(&message.hash()[..])
					.map_err(|_| internal_err("invalid signing message"))?;
				let (signature, recid) = secp256k1::sign(&signing_message, secret);

				let v = match message.chain_id {
					None => 27 + recid.serialize() as u64,
					Some(chain_id) => 2 * chain_id + 35 + recid.serialize() as u64,
				};
				let rs = signature.serialize();
				let r = H256::from_slice(&rs[0..32]);
				let s = H256::from_slice(&rs[32..64]);

				transaction = Some(ethereum::Transaction {
					nonce: message.nonce,
					gas_price: message.gas_price,
					gas_limit: message.gas_limit,
					action: message.action,
					value: message.value,
					input: message.input.clone(),
					signature: ethereum::TransactionSignature::new(v, r, s)
						.ok_or(internal_err("signer generated invalid signature"))?,
				});

				break;
			}
		}

		transaction.ok_or(internal_err("signer not available"))
	}
}
