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

use ethereum::TransactionV2 as EthereumTransaction;
use ethereum_types::{H160, H256};
use jsonrpsee::types::ErrorObjectOwned;
// Substrate
use sp_core::hashing::keccak_256;
// Frontier
use fc_rpc_core::types::TransactionMessage;

use crate::internal_err;

/// A generic Ethereum signer.
pub trait EthSigner: Send + Sync {
	/// Available accounts from this signer.
	fn accounts(&self) -> Vec<H160>;
	/// Sign a transaction message using the given account in message.
	fn sign(
		&self,
		message: TransactionMessage,
		address: &H160,
	) -> Result<EthereumTransaction, ErrorObjectOwned>;
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
	H160::from(H256::from(keccak_256(&res)))
}

impl EthSigner for EthDevSigner {
	fn accounts(&self) -> Vec<H160> {
		self.keys.iter().map(secret_key_address).collect()
	}

	fn sign(
		&self,
		message: TransactionMessage,
		address: &H160,
	) -> Result<EthereumTransaction, ErrorObjectOwned> {
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
								input: m.input,
								signature: ethereum::TransactionSignature::new(v, r, s)
									.ok_or_else(|| {
										internal_err("signer generated invalid signature")
									})?,
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

		transaction.ok_or_else(|| internal_err("signer not available"))
	}
}
