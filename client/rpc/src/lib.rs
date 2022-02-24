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

pub use self::{
	eth::{
		EthApi, EthApiServer, EthBlockDataCache, EthFilterApi, EthFilterApiServer, EthTask, NetApi,
		NetApiServer, Web3Api, Web3ApiServer,
	},
	eth_pubsub::{EthPubSubApi, EthPubSubApiServer, HexEncodedIdProvider},
	overrides::{
		OverrideHandle, RuntimeApiStorageOverride, SchemaV1Override, SchemaV2Override,
		SchemaV3Override, StorageOverride,
	},
};

pub use ethereum::TransactionV2 as EthereumTransaction;
use ethereum_types::{H160, H256};
use evm::{ExitError, ExitReason};
pub use fc_rpc_core::types::TransactionMessage;
use jsonrpc_core::{Error, ErrorCode, Value};
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
	use fp_storage::EthereumStorageSchema;

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
		only_canonical: bool,
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
				data: Some(Value::String(hex::encode(data))),
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

/// A generic Ethereum signer.
pub trait EthSigner: Send + Sync {
	/// Available accounts from this signer.
	fn accounts(&self) -> Vec<H160>;
	/// Sign a transaction message using the given account in message.
	fn sign(
		&self,
		message: TransactionMessage,
		address: &H160,
	) -> Result<EthereumTransaction, Error>;
}

pub struct EthDevSigner {
	keys: Vec<libsecp256k1::SecretKey>,
}

impl EthDevSigner {
	pub fn new() -> Self {
		Self {
			keys: vec![libsecp256k1::SecretKey::parse(&[
				0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
				0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
				0x11, 0x11, 0x11, 0x11,
			])
			.expect("Test key is valid; qed")],
		}
	}
}

fn secret_key_address(secret: &libsecp256k1::SecretKey) -> H160 {
	let public = libsecp256k1::PublicKey::from_secret_key(secret);
	public_key_address(&public)
}

fn public_key_address(public: &libsecp256k1::PublicKey) -> H160 {
	let mut res = [0u8; 64];
	res.copy_from_slice(&public.serialize()[1..65]);
	H160::from(H256::from_slice(Keccak256::digest(&res).as_slice()))
}

impl EthSigner for EthDevSigner {
	fn accounts(&self) -> Vec<H160> {
		self.keys.iter().map(secret_key_address).collect()
	}

	fn sign(
		&self,
		message: TransactionMessage,
		address: &H160,
	) -> Result<EthereumTransaction, Error> {
		let mut transaction = None;

		for secret in &self.keys {
			let key_address = secret_key_address(secret);

			if &key_address == address {
				match message {
					TransactionMessage::Legacy(m) => {
						let signing_message = libsecp256k1::Message::parse_slice(&m.hash()[..])
							.map_err(|_| internal_err("invalid signing message"))?;
						let (signature, recid) = libsecp256k1::sign(&signing_message, secret);
						let v = match m.chain_id {
							None => 27 + recid.serialize() as u64,
							Some(chain_id) => 2 * chain_id + 35 + recid.serialize() as u64,
						};
						let rs = signature.serialize();
						let r = H256::from_slice(&rs[0..32]);
						let s = H256::from_slice(&rs[32..64]);
						transaction =
							Some(EthereumTransaction::Legacy(ethereum::LegacyTransaction {
								nonce: m.nonce,
								gas_price: m.gas_price,
								gas_limit: m.gas_limit,
								action: m.action,
								value: m.value,
								input: m.input.clone(),
								signature: ethereum::TransactionSignature::new(v, r, s)
									.ok_or(internal_err("signer generated invalid signature"))?,
							}));
					}
					TransactionMessage::EIP2930(m) => {
						let signing_message = libsecp256k1::Message::parse_slice(&m.hash()[..])
							.map_err(|_| internal_err("invalid signing message"))?;
						let (signature, recid) = libsecp256k1::sign(&signing_message, secret);
						let rs = signature.serialize();
						let r = H256::from_slice(&rs[0..32]);
						let s = H256::from_slice(&rs[32..64]);
						transaction =
							Some(EthereumTransaction::EIP2930(ethereum::EIP2930Transaction {
								chain_id: m.chain_id,
								nonce: m.nonce,
								gas_price: m.gas_price,
								gas_limit: m.gas_limit,
								action: m.action,
								value: m.value,
								input: m.input.clone(),
								access_list: m.access_list,
								odd_y_parity: recid.serialize() != 0,
								r,
								s,
							}));
					}
					TransactionMessage::EIP1559(m) => {
						let signing_message = libsecp256k1::Message::parse_slice(&m.hash()[..])
							.map_err(|_| internal_err("invalid signing message"))?;
						let (signature, recid) = libsecp256k1::sign(&signing_message, secret);
						let rs = signature.serialize();
						let r = H256::from_slice(&rs[0..32]);
						let s = H256::from_slice(&rs[32..64]);
						transaction =
							Some(EthereumTransaction::EIP1559(ethereum::EIP1559Transaction {
								chain_id: m.chain_id,
								nonce: m.nonce,
								max_priority_fee_per_gas: m.max_priority_fee_per_gas,
								max_fee_per_gas: m.max_fee_per_gas,
								gas_limit: m.gas_limit,
								action: m.action,
								value: m.value,
								input: m.input.clone(),
								access_list: m.access_list,
								odd_y_parity: recid.serialize() != 0,
								r,
								s,
							}));
					}
				}
				break;
			}
		}

		transaction.ok_or(internal_err("signer not available"))
	}
}
