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

use ethereum_types::H256;
use futures::future::TryFutureExt;
use jsonrpsee::core::RpcResult;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_core::H160;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::{traits::Block as BlockT, transaction_validity::TransactionSource};
// Frontier
use fc_rpc_core::types::{TransactionRequest, DEFAULT_MAX_TX_INPUT_BYTES, *};
use fp_rpc::{ConvertTransaction, ConvertTransactionRuntimeApi, EthereumRuntimeRPCApi};

use crate::{
	eth::{format, Eth},
	internal_err, public_key,
};

fn is_unprotected_legacy_tx(transaction: &ethereum::TransactionV3) -> bool {
	matches!(
		transaction,
		ethereum::TransactionV3::Legacy(tx) if tx.signature.chain_id().is_none()
	)
}

fn should_reject_unprotected_legacy_tx(
	transaction: &ethereum::TransactionV3,
	allow_unprotected_txs: bool,
) -> bool {
	is_unprotected_legacy_tx(transaction) && !allow_unprotected_txs
}

impl<B, C, P, CT, BE, CIDP, EC> Eth<B, C, P, CT, BE, CIDP, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + ConvertTransactionRuntimeApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	P: TransactionPool<Block = B, Hash = B::Hash> + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + 'static,
	CIDP: CreateInherentDataProviders<B, ()> + Send + 'static,
{
	fn ensure_unprotected_legacy_tx_allowed(
		&self,
		transaction: &ethereum::TransactionV3,
	) -> RpcResult<()> {
		if should_reject_unprotected_legacy_tx(transaction, self.rpc_allow_unprotected_txs) {
			// Runtime validation remains authoritative since peers can still gossip transactions
			// over p2p and bypass RPC submission.
			return Err(internal_err(
				"unprotected legacy transactions are not allowed by RPC policy",
			));
		}

		Ok(())
	}

	pub async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<H256> {
		request
			.validate_size()
			.map_err(|msg| crate::err(jsonrpsee::types::error::INVALID_PARAMS_CODE, &msg, None))?;

		let from = match request.from {
			Some(from) => from,
			None => {
				let accounts = match self.accounts() {
					Ok(accounts) => accounts,
					Err(e) => return Err(e),
				};

				match accounts.first() {
					Some(account) => *account,
					None => return Err(internal_err("no signer available")),
				}
			}
		};

		let nonce = match request.nonce {
			Some(nonce) => nonce,
			None => match self.transaction_count(from, None).await {
				Ok(nonce) => nonce,
				Err(e) => return Err(e),
			},
		};

		let chain_id = match (request.chain_id, self.chain_id()) {
			(Some(id), Ok(Some(chain_id))) if id != chain_id => {
				return Err(internal_err("chain id is mismatch"))
			}
			(_, Ok(Some(chain_id))) => chain_id.as_u64(),
			(_, Ok(None)) => return Err(internal_err("chain id not available")),
			(_, Err(e)) => return Err(e),
		};

		let block_hash = self.client.info().best_hash;

		let gas_price = request.gas_price;
		let gas_limit = match request.gas {
			Some(gas_limit) => gas_limit,
			None => {
				if let Ok(Some(block)) = self.client.runtime_api().current_block(block_hash) {
					block.header.gas_limit
				} else {
					return Err(internal_err("block unavailable, cannot query gas limit"));
				}
			}
		};

		let max_fee_per_gas = request.max_fee_per_gas;
		let message: Option<TransactionMessage> = request.into();
		let message = match message {
			Some(TransactionMessage::Legacy(mut m)) => {
				m.nonce = nonce;
				m.chain_id = Some(chain_id);
				m.gas_limit = gas_limit;
				if gas_price.is_none() {
					m.gas_price = self.gas_price().unwrap_or_default();
				}
				TransactionMessage::Legacy(m)
			}
			Some(TransactionMessage::EIP2930(mut m)) => {
				m.nonce = nonce;
				m.chain_id = chain_id;
				m.gas_limit = gas_limit;
				if gas_price.is_none() {
					m.gas_price = self.gas_price().unwrap_or_default();
				}
				TransactionMessage::EIP2930(m)
			}
			Some(TransactionMessage::EIP1559(mut m)) => {
				m.nonce = nonce;
				m.chain_id = chain_id;
				m.gas_limit = gas_limit;
				if max_fee_per_gas.is_none() {
					m.max_fee_per_gas = self.gas_price().unwrap_or_default();
				}
				TransactionMessage::EIP1559(m)
			}
			_ => return Err(internal_err("invalid transaction parameters")),
		};

		let mut transaction = None;
		for signer in &self.signers {
			if signer.accounts().contains(&from) {
				match signer.sign(message, &from) {
					Ok(t) => transaction = Some(t),
					Err(e) => return Err(e),
				}
				break;
			}
		}

		let transaction = match transaction {
			Some(transaction) => transaction,
			None => return Err(internal_err("no signer available")),
		};
		self.ensure_unprotected_legacy_tx_allowed(&transaction)?;
		let transaction_hash = transaction.hash();

		let extrinsic = self.convert_transaction(block_hash, transaction)?;

		self.pool
			.submit_one(block_hash, TransactionSource::Local, extrinsic)
			.map_ok(move |_| transaction_hash)
			.map_err(|err| internal_err(format::Geth::pool_error(err)))
			.await
	}

	pub async fn send_raw_transaction(&self, bytes: Bytes) -> RpcResult<H256> {
		let bytes = bytes.into_vec();
		if bytes.is_empty() {
			return Err(internal_err("transaction data is empty"));
		}

		// Validate transaction size to prevent DoS attacks.
		// This matches geth/reth pool validation which rejects transactions > 128 KB.
		if bytes.len() > DEFAULT_MAX_TX_INPUT_BYTES {
			return Err(crate::err(
				jsonrpsee::types::error::INVALID_PARAMS_CODE,
				format!(
					"oversized data: transaction size {} exceeds limit {DEFAULT_MAX_TX_INPUT_BYTES}",
					bytes.len()
				),
				None,
			));
		}

		let transaction: ethereum::TransactionV3 =
			match ethereum::EnvelopedDecodable::decode(&bytes) {
				Ok(transaction) => transaction,
				Err(_) => return Err(internal_err("decode transaction failed")),
			};
		self.ensure_unprotected_legacy_tx_allowed(&transaction)?;
		let transaction_hash = transaction.hash();

		let block_hash = self.client.info().best_hash;
		let extrinsic = self.convert_transaction(block_hash, transaction)?;

		self.pool
			.submit_one(block_hash, TransactionSource::Local, extrinsic)
			.map_ok(move |_| transaction_hash)
			.map_err(|err| internal_err(format::Geth::pool_error(err)))
			.await
	}

	pub async fn pending_transactions(&self) -> RpcResult<Vec<Transaction>> {
		let ready = self
			.pool
			.ready()
			.map(|in_pool_tx| in_pool_tx.data().as_ref().clone())
			.collect::<Vec<_>>();

		let future = self
			.pool
			.futures()
			.iter()
			.map(|in_pool_tx| in_pool_tx.data().as_ref().clone())
			.collect::<Vec<_>>();

		let all_extrinsics = ready
			.iter()
			.chain(future.iter())
			.cloned()
			.collect::<Vec<_>>();

		let best_block = self.client.info().best_hash;
		let api = self.client.runtime_api();

		let api_version = api
			.api_version::<dyn EthereumRuntimeRPCApi<B>>(best_block)
			.map_err(|err| internal_err(format!("Failed to get API version: {err}")))?
			.ok_or_else(|| internal_err("Failed to get API version"))?;

		let ethereum_txs = if api_version > 1 {
			api.extrinsic_filter(best_block, all_extrinsics)
				.map_err(|err| internal_err(format!("Runtime call failed: {err}")))?
		} else {
			#[allow(deprecated)]
			let legacy = api
				.extrinsic_filter_before_version_2(best_block, all_extrinsics)
				.map_err(|err| internal_err(format!("Runtime call failed: {err}")))?;
			legacy.into_iter().map(|tx| tx.into()).collect()
		};

		let transactions = ethereum_txs
			.into_iter()
			.filter_map(|tx| {
				let pubkey = match public_key(&tx) {
					Ok(pk) => H160::from(H256::from(sp_core::hashing::keccak_256(&pk))),
					Err(_err) => {
						// Skip transactions with invalid public keys
						return None;
					}
				};

				Some(Transaction::build_from(pubkey, &tx))
			})
			.collect();

		Ok(transactions)
	}

	fn convert_transaction(
		&self,
		block_hash: B::Hash,
		transaction: ethereum::TransactionV3,
	) -> RpcResult<B::Extrinsic> {
		let api_version = match self
			.client
			.runtime_api()
			.api_version::<dyn ConvertTransactionRuntimeApi<B>>(block_hash)
		{
			Ok(api_version) => api_version,
			_ => return Err(internal_err("cannot access `ConvertTransactionRuntimeApi`")),
		};

		match api_version {
			Some(2) => match self
				.client
				.runtime_api()
				.convert_transaction(block_hash, transaction)
			{
				Ok(extrinsic) => Ok(extrinsic),
				Err(_) => Err(internal_err("cannot access `ConvertTransactionRuntimeApi`")),
			},
			Some(1) => {
				if let ethereum::TransactionV3::Legacy(legacy_transaction) = transaction {
					// To be compatible with runtimes that do not support transactions v2
					#[allow(deprecated)]
					match self
						.client
						.runtime_api()
						.convert_transaction_before_version_2(block_hash, legacy_transaction)
					{
						Ok(extrinsic) => Ok(extrinsic),
						Err(_) => Err(internal_err("cannot access `ConvertTransactionRuntimeApi`")),
					}
				} else {
					Err(internal_err(
						"Ethereum transactions v2 is not supported by the runtime",
					))
				}
			}
			None => {
				if let Some(ref convert_transaction) = self.convert_transaction {
					Ok(convert_transaction.convert_transaction(transaction.clone()))
				} else {
					Err(internal_err(
						"`ConvertTransactionRuntimeApi` is not found and no `TransactionConverter` is provided"
					))
				}
			}
			_ => Err(internal_err(
				"`ConvertTransactionRuntimeApi` is not supported",
			)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{is_unprotected_legacy_tx, should_reject_unprotected_legacy_tx};
	use ethereum::{
		legacy::TransactionSignature as LegacyTransactionSignature, LegacyTransaction,
		TransactionAction, TransactionV3,
	};
	use rlp::RlpStream;
	use sp_core::{hashing::keccak_256, H160, H256, U256};

	fn legacy_tx_with_v(v: u64) -> TransactionV3 {
		let nonce = U256::zero();
		let gas_price = U256::from(1u8);
		let gas_limit = U256::from(21_000u64);
		let action = TransactionAction::Call(H160::default());
		let value = U256::zero();
		let input = Vec::new();

		let mut stream = RlpStream::new_list(6);
		stream.append(&nonce);
		stream.append(&gas_price);
		stream.append(&gas_limit);
		stream.append(&action);
		stream.append(&value);
		stream.append(&input);
		let hash = H256::from(keccak_256(&stream.out()));
		let msg = libsecp256k1::Message::parse(hash.as_fixed_bytes());
		let secret = libsecp256k1::SecretKey::parse_slice(&[1u8; 32]).expect("valid secret key");
		let (signature, _) = libsecp256k1::sign(&msg, &secret);
		let rs = signature.serialize();

		TransactionV3::Legacy(LegacyTransaction {
			nonce,
			gas_price,
			gas_limit,
			action,
			value,
			input,
			signature: LegacyTransactionSignature::new(
				v,
				H256::from_slice(&rs[0..32]),
				H256::from_slice(&rs[32..64]),
			)
			.expect("valid legacy signature"),
		})
	}

	#[test]
	fn detects_unprotected_legacy_transaction() {
		let tx = legacy_tx_with_v(27);

		assert!(is_unprotected_legacy_tx(&tx));
	}

	#[test]
	fn ignores_protected_legacy_transaction() {
		let tx = legacy_tx_with_v(37);

		assert!(!is_unprotected_legacy_tx(&tx));
	}

	#[test]
	fn reject_decision_respects_policy() {
		let unprotected_legacy = legacy_tx_with_v(27);
		assert!(should_reject_unprotected_legacy_tx(
			&unprotected_legacy,
			false
		));
		assert!(!should_reject_unprotected_legacy_tx(
			&unprotected_legacy,
			true
		));
	}
}
