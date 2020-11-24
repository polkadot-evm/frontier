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

pub use eth::{EthApi, EthApiServer, NetApi, NetApiServer, Web3Api, Web3ApiServer};
pub use eth_pubsub::{EthPubSubApi, EthPubSubApiServer, HexEncodedIdProvider};

use ethereum_types::{H160, H256};
use jsonrpc_core::{ErrorCode, Error, Value};
use rustc_hex::ToHex;
use pallet_evm::ExitReason;
use sha3::{Digest, Keccak256};

pub fn internal_err<T: ToString>(message: T) -> Error {
	Error {
		code: ErrorCode::InternalError,
		message: message.to_string(),
		data: None
	}
}

pub fn error_on_execution_failure(reason: &ExitReason, data: &[u8]) -> Result<(), Error> {
	match reason {
		ExitReason::Succeed(_) => Ok(()),
		ExitReason::Error(e) => {
			Err(Error {
				code: ErrorCode::InternalError,
				message: format!("evm error: {:?}", e),
				data: Some(Value::String("0x".to_string()))
			})
		},
		ExitReason::Revert(e) => {
			Err(Error {
				code: ErrorCode::InternalError,
				message: format!("evm revert: {:?}", e),
				data: Some(Value::String(data.to_hex()))
			})
		},
		ExitReason::Fatal(e) => {
			Err(Error {
				code: ErrorCode::InternalError,
				message: format!("evm fatal: {:?}", e),
				data: Some(Value::String("0x".to_string()))
			})
		},
	}
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
			keys: vec![
				secp256k1::SecretKey::parse(&[
					0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
					0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
					0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
					0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
				]).expect("Test key is valid; qed"),
			],
		}
	}
}

impl EthSigner for EthDevSigner {
	fn accounts(&self) -> Vec<H160> {
		self.keys.iter().map(|secret| {
			let public = secp256k1::PublicKey::from_secret_key(secret);
			let mut res = [0u8; 64];
			res.copy_from_slice(&public.serialize()[1..65]);

			H160::from(H256::from_slice(Keccak256::digest(&res).as_slice()))
		}).collect()
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

				break
			}
		}

		transaction.ok_or(internal_err("signer not available"))
	}
}
