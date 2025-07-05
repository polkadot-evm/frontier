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

use ethereum::{eip2930, legacy, TransactionV3 as EthereumTransaction};
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
								signature: legacy::TransactionSignature::new(v, r, s).ok_or_else(
									|| internal_err("signer generated invalid signature"),
								)?,
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
								signature: eip2930::TransactionSignature::new(
									recid.serialize() != 0,
									r,
									s,
								)
								.ok_or(internal_err("Invalid transaction signature format"))?,
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
								signature: eip2930::TransactionSignature::new(
									recid.serialize() != 0,
									r,
									s,
								)
								.ok_or(internal_err("Invalid transaction signature format"))?,
							}));
					}
					TransactionMessage::EIP7702(m) => {
						let signing_message = libsecp256k1::Message::parse_slice(&m.hash()[..])
							.map_err(|_| internal_err("invalid signing message"))?;
						let (signature, recid) = libsecp256k1::sign(&signing_message, secret);
						let rs = signature.serialize();
						let r = H256::from_slice(&rs[0..32]);
						let s = H256::from_slice(&rs[32..64]);
						transaction =
							Some(EthereumTransaction::EIP7702(ethereum::EIP7702Transaction {
								chain_id: m.chain_id,
								nonce: m.nonce,
								max_priority_fee_per_gas: m.max_priority_fee_per_gas,
								max_fee_per_gas: m.max_fee_per_gas,
								gas_limit: m.gas_limit,
								destination: m.destination,
								value: m.value,
								data: m.data.clone(),
								access_list: m.access_list,
								authorization_list: m.authorization_list,
								signature: eip2930::TransactionSignature::new(
									recid.serialize() != 0,
									r,
									s,
								)
								.ok_or(internal_err("Invalid transaction signature format"))?,
							}));
					}
				}
				break;
			}
		}

		transaction.ok_or_else(|| internal_err("signer not available"))
	}
}
