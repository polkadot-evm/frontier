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

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

extern crate alloc;

use alloc::{vec, vec::Vec};
pub use ethereum::{
	AccessListItem, AuthorizationList, AuthorizationListItem, BlockV3 as Block,
	LegacyTransactionMessage, Log, ReceiptV3 as Receipt, TransactionAction,
	TransactionV3 as Transaction,
};
use ethereum_types::{H160, H256, U256};
use fp_evm::{CallOrCreateInfo, CheckEvmTransactionInput};
use frame_support::dispatch::{DispatchErrorWithPostInfo, PostDispatchInfo};
use scale_codec::{Decode, Encode};

/// EIP-7702 Authorization tuple
#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
pub struct Authorization {
	/// Chain ID
	pub chain_id: U256,
	/// Address to delegate to
	pub address: H160,
	/// Nonce for the authorization
	pub nonce: U256,
	/// Y parity for signature recovery
	pub y_parity: bool,
	/// R component of signature
	pub r: U256,
	/// S component of signature
	pub s: U256,
}

impl Authorization {
	/// Process authorization list and return valid authorizations with their signers
	pub fn process_authorization_list(
		authorizations: &[Authorization],
		current_chain_id: u64,
	) -> Vec<(H160, H160)> {
		let mut processed = Vec::new();

		for auth in authorizations {
			if let Ok(signer) = auth.recover_signer(current_chain_id) {
				// Only add valid authorizations
				// Returns (signer_address, authorized_address) tuples
				processed.push((signer, auth.address));
			}
			// Invalid authorizations are silently ignored per EIP-7702
		}

		processed
	}

	/// Create delegation designator code for EIP-7702
	/// Format: 0xef0100 + 20 bytes of authorized address
	pub fn create_delegation_designator(authorized_address: H160) -> Vec<u8> {
		let mut code = vec![0xef, 0x01, 0x00];
		code.extend_from_slice(authorized_address.as_bytes());
		code
	}

	/// Apply authorization list to EVM state by setting delegation designators
	pub fn apply_authorization_list<F>(
		authorizations: &[Authorization],
		current_chain_id: u64,
		mut set_code_fn: F,
	) -> Result<(), &'static str>
	where
		F: FnMut(H160, Vec<u8>) -> Result<(), &'static str>,
	{
		let processed = Self::process_authorization_list(authorizations, current_chain_id);

		for (signer_address, authorized_address) in processed {
			// Create delegation designator code
			let delegation_code = Self::create_delegation_designator(authorized_address);

			// Set the signer's account code to the delegation designator
			set_code_fn(signer_address, delegation_code)?;
		}

		Ok(())
	}

	/// Validate authorization signature and return signer address
	pub fn recover_signer(&self, current_chain_id: u64) -> Result<H160, AuthorizationError> {
		// Chain ID must be 0 or current chain ID
		if self.chain_id != U256::zero() && self.chain_id != U256::from(current_chain_id) {
			return Err(AuthorizationError::InvalidChainId);
		}

		// Nonce must be < 2^64
		if self.nonce >= U256::from(1u128 << 64) {
			return Err(AuthorizationError::NonceOverflow);
		}

		// s must be <= secp256k1n/2 for canonical signatures
		let secp256k1n_half = U256::from_dec_str(
			"57896044618658097711785492504343953926418782139537452191302581570759080747168",
		)
		.unwrap();

		if self.s > secp256k1n_half {
			return Err(AuthorizationError::InvalidSignature);
		}

		// Perform ECDSA recovery to get signer address
		self.ecrecover_signer(current_chain_id)
	}

	/// Perform ECDSA recovery to get the signer address
	fn ecrecover_signer(&self, current_chain_id: u64) -> Result<H160, AuthorizationError> {
		// Create the authorization message hash according to EIP-7702
		let message_hash = self.authorization_message_hash(current_chain_id);

		// Convert signature components to the format expected by sp_io::crypto
		let mut signature = [0u8; 65];

		// Convert U256 to big endian bytes
		let r_bytes = self.r.to_big_endian();
		let s_bytes = self.s.to_big_endian();

		signature[0..32].copy_from_slice(&r_bytes); // r
		signature[32..64].copy_from_slice(&s_bytes); // s
		signature[64] = if self.y_parity { 1 } else { 0 }; // recovery_id

		// Use Substrate's built-in ecrecover
		let message_bytes: [u8; 32] = message_hash.into();
		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&signature, &message_bytes)
			.map_err(|_| AuthorizationError::EcrecoverFailed)?;

		// Convert public key to Ethereum address using Substrate's keccak
		let address_hash = sp_io::hashing::keccak_256(&pubkey);

		// Ethereum address is the last 20 bytes of the keccak256 hash
		Ok(H160::from_slice(&address_hash[12..]))
	}

	/// Create the authorization message hash according to EIP-7702
	fn authorization_message_hash(&self, current_chain_id: u64) -> H256 {
		// EIP-7702 authorization message format:
		// MAGIC || rlp([chain_id, address, nonce])
		// EIP-7702 authorization magic is 0x05
		let mut message = alloc::vec![0x05];

		// Use current chain ID if authorization chain ID is 0
		let effective_chain_id = if self.chain_id == U256::zero() {
			U256::from(current_chain_id)
		} else {
			self.chain_id
		};

		// RLP encode the authorization tuple
		let mut rlp_stream = rlp::RlpStream::new_list(3);
		rlp_stream.append(&effective_chain_id);
		rlp_stream.append(&self.address);
		rlp_stream.append(&self.nonce);
		message.extend_from_slice(&rlp_stream.out());

		// Return keccak256 hash of the complete message using Substrate's keccak
		H256::from(sp_io::hashing::keccak_256(&message))
	}
}

impl From<Authorization> for AuthorizationListItem {
	fn from(auth: Authorization) -> Self {
		AuthorizationListItem {
			chain_id: auth.chain_id.as_u64(),
			address: auth.address,
			nonce: auth.nonce,
			y_parity: auth.y_parity,
			r: H256::from(auth.r.to_big_endian()),
			s: H256::from(auth.s.to_big_endian()),
		}
	}
}

impl From<AuthorizationListItem> for Authorization {
	fn from(item: AuthorizationListItem) -> Self {
		Authorization {
			chain_id: U256::from(item.chain_id),
			address: item.address,
			nonce: item.nonce,
			y_parity: item.y_parity,
			r: U256::from_big_endian(&item.r[..]),
			s: U256::from_big_endian(&item.s[..]),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_authorization_validation() {
		let auth = Authorization {
			chain_id: U256::from(1),
			address: H160::zero(),
			nonce: U256::from(42),
			y_parity: false,
			r: U256::from(123),
			s: U256::from(456),
		};

		// Test valid chain ID
		let result = auth.recover_signer(1);
		assert!(result.is_ok());

		// Test invalid chain ID
		let result = auth.recover_signer(2);
		assert!(matches!(
			result.err(),
			Some(AuthorizationError::InvalidChainId)
		));
	}

	#[test]
	fn test_authorization_chain_id_zero() {
		let auth = Authorization {
			chain_id: U256::zero(), // Zero means any chain
			address: H160::zero(),
			nonce: U256::from(42),
			y_parity: false,
			r: U256::from(123),
			s: U256::from(456),
		};

		// Should work with any chain ID when authorization chain_id is 0
		let result = auth.recover_signer(1);
		assert!(result.is_ok());

		let result = auth.recover_signer(999);
		assert!(result.is_ok());
	}

	#[test]
	fn test_authorization_nonce_overflow() {
		let auth = Authorization {
			chain_id: U256::from(1),
			address: H160::zero(),
			nonce: U256::from(1u128 << 64), // Over the limit
			y_parity: false,
			r: U256::from(123),
			s: U256::from(456),
		};

		let result = auth.recover_signer(1);
		assert!(matches!(
			result.err(),
			Some(AuthorizationError::NonceOverflow)
		));
	}

	#[test]
	fn test_authorization_invalid_s_value() {
		let auth = Authorization {
			chain_id: U256::from(1),
			address: H160::zero(),
			nonce: U256::from(42),
			y_parity: false,
			r: U256::from(123),
			// s value greater than secp256k1n/2
			s: U256::from_dec_str(
				"57896044618658097711785492504343953926418782139537452191302581570759080747169",
			)
			.unwrap(),
		};

		let result = auth.recover_signer(1);
		assert!(matches!(
			result.err(),
			Some(AuthorizationError::InvalidSignature)
		));
	}

	#[test]
	fn test_authorization_message_hash() {
		let auth = Authorization {
			chain_id: U256::from(1),
			address: H160::from_slice(&[1u8; 20]),
			nonce: U256::from(42),
			y_parity: false,
			r: U256::from(123),
			s: U256::from(456),
		};

		let hash1 = auth.authorization_message_hash(1);
		let hash2 = auth.authorization_message_hash(1);

		// Same parameters should produce same hash
		assert_eq!(hash1, hash2);

		// Test with chain_id = 0 (should use current_chain_id)
		let auth_zero_chain = Authorization {
			chain_id: U256::zero(),
			address: H160::from_slice(&[1u8; 20]),
			nonce: U256::from(42),
			y_parity: false,
			r: U256::from(123),
			s: U256::from(456),
		};

		let hash3 = auth_zero_chain.authorization_message_hash(1);
		let hash4 = auth_zero_chain.authorization_message_hash(2);

		// Different current chain ID should produce different hash when auth chain_id is 0
		assert_ne!(hash3, hash4);
	}
}

/// Authorization validation errors
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationError {
	InvalidChainId,
	NonceOverflow,
	InvalidSignature,
	EcrecoverFailed,
}

pub trait ValidatedTransaction {
	fn apply(
		source: H160,
		transaction: Transaction,
	) -> Result<(PostDispatchInfo, CallOrCreateInfo), DispatchErrorWithPostInfo>;
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
pub struct TransactionData {
	pub action: TransactionAction,
	pub input: Vec<u8>,
	pub nonce: U256,
	pub gas_limit: U256,
	pub gas_price: Option<U256>,
	pub max_fee_per_gas: Option<U256>,
	pub max_priority_fee_per_gas: Option<U256>,
	pub value: U256,
	pub chain_id: Option<u64>,
	pub access_list: Vec<(H160, Vec<H256>)>,
	pub authorization_list: AuthorizationList,
}

impl TransactionData {
	#[allow(clippy::too_many_arguments)]
	pub fn new(
		action: TransactionAction,
		input: Vec<u8>,
		nonce: U256,
		gas_limit: U256,
		gas_price: Option<U256>,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		value: U256,
		chain_id: Option<u64>,
		access_list: Vec<(H160, Vec<H256>)>,
		authorization_list: AuthorizationList,
	) -> Self {
		Self {
			action,
			input,
			nonce,
			gas_limit,
			gas_price,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			value,
			chain_id,
			access_list,
			authorization_list,
		}
	}

	// The transact call wrapped in the extrinsic is part of the PoV, record this as a base cost for the size of the proof.
	pub fn proof_size_base_cost(&self) -> u64 {
		self.encode()
			.len()
			// signature
			.saturating_add(65)
			// pallet index
			.saturating_add(1)
			// call index
			.saturating_add(1) as u64
	}
}

impl From<TransactionData> for CheckEvmTransactionInput {
	fn from(t: TransactionData) -> Self {
		CheckEvmTransactionInput {
			to: if let TransactionAction::Call(to) = t.action {
				Some(to)
			} else {
				None
			},
			chain_id: t.chain_id,
			input: t.input,
			nonce: t.nonce,
			gas_limit: t.gas_limit,
			gas_price: t.gas_price,
			max_fee_per_gas: t.max_fee_per_gas,
			max_priority_fee_per_gas: t.max_priority_fee_per_gas,
			value: t.value,
			access_list: t.access_list,
			authorization_list: t.authorization_list,
		}
	}
}

impl From<&Transaction> for TransactionData {
	fn from(t: &Transaction) -> Self {
		match t {
			Transaction::Legacy(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				value: t.value,
				chain_id: t.signature.chain_id(),
				access_list: Vec::new(),
				authorization_list: Vec::new(),
			},
			Transaction::EIP2930(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				value: t.value,
				chain_id: Some(t.chain_id),
				access_list: t
					.access_list
					.iter()
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
				authorization_list: Vec::new(),
			},
			Transaction::EIP1559(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: None,
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				value: t.value,
				chain_id: Some(t.chain_id),
				access_list: t
					.access_list
					.iter()
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
				authorization_list: Vec::new(),
			},
			Transaction::EIP7702(t) => TransactionData {
				action: t.destination,
				input: t.data.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: None,
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				value: t.value,
				chain_id: Some(t.chain_id),
				access_list: t
					.access_list
					.iter()
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
				authorization_list: t
					.authorization_list
					.iter()
					.map(|d| (d.chain_id, d.address, d.nonce, d.authorizing_address()))
					.collect(),
			},
		}
	}
}
