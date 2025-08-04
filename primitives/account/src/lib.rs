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

use alloc::string::{String, ToString};
use core::fmt;

use scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
// Substrate
use sp_core::{crypto::AccountId32, ecdsa, RuntimeDebug, H160, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::MultiSignature;

// Polkadot / XCM
use xcm::latest::{Junction, Location};

/// A fully Ethereum-compatible `AccountId`.
/// Conforms to H160 address and ECDSA key standards.
/// Alternative to H256->H160 mapping.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default, Hash)]
#[derive(Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo)]
pub struct AccountId20(pub [u8; 20]);

#[cfg(feature = "serde")]
impl_serde::impl_fixed_hash_serde!(AccountId20, 20);

#[cfg(feature = "std")]
impl std::str::FromStr for AccountId20 {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		H160::from_str(s)
			.map(Into::into)
			.map_err(|_| "invalid hex address.")
	}
}

impl fmt::Display for AccountId20 {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let address = hex::encode(self.0).trim_start_matches("0x").to_lowercase();
		let address_hash = hex::encode(keccak_256(address.as_bytes()));

		let checksum: String =
			address
				.char_indices()
				.fold(String::from("0x"), |mut acc, (index, address_char)| {
					let n = u16::from_str_radix(&address_hash[index..index + 1], 16)
						.expect("Keccak256 hashed; qed");

					if n > 7 {
						// make char uppercase if ith character is 9..f
						acc.push_str(&address_char.to_uppercase().to_string())
					} else {
						// already lowercased
						acc.push(address_char)
					}

					acc
				});
		write!(f, "{checksum}")
	}
}

impl fmt::Debug for AccountId20 {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{:?}", H160(self.0))
	}
}

impl From<[u8; 20]> for AccountId20 {
	fn from(bytes: [u8; 20]) -> Self {
		Self(bytes)
	}
}

impl<'a> TryFrom<&'a [u8]> for AccountId20 {
	type Error = ();
	fn try_from(x: &'a [u8]) -> Result<AccountId20, ()> {
		if x.len() == 20 {
			let mut data = [0; 20];
			data.copy_from_slice(x);
			Ok(AccountId20(data))
		} else {
			Err(())
		}
	}
}

impl From<AccountId20> for [u8; 20] {
	fn from(val: AccountId20) -> Self {
		val.0
	}
}

impl From<H160> for AccountId20 {
	fn from(h160: H160) -> Self {
		Self(h160.0)
	}
}

impl From<AccountId20> for H160 {
	fn from(val: AccountId20) -> Self {
		H160(val.0)
	}
}

impl AsRef<[u8]> for AccountId20 {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

impl AsMut<[u8]> for AccountId20 {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0[..]
	}
}

impl AsRef<[u8; 20]> for AccountId20 {
	fn as_ref(&self) -> &[u8; 20] {
		&self.0
	}
}

impl AsMut<[u8; 20]> for AccountId20 {
	fn as_mut(&mut self) -> &mut [u8; 20] {
		&mut self.0
	}
}

impl From<ecdsa::Public> for AccountId20 {
	fn from(pk: ecdsa::Public) -> Self {
		let decompressed = libsecp256k1::PublicKey::parse_compressed(&pk.0)
			.expect("Wrong compressed public key provided")
			.serialize();
		let mut m = [0u8; 64];
		m.copy_from_slice(&decompressed[1..65]);
		let account = H160::from(H256::from(keccak_256(&m)));
		Self(account.into())
	}
}

impl From<[u8; 32]> for AccountId20 {
	fn from(bytes: [u8; 32]) -> Self {
		let mut buffer = [0u8; 20];
		buffer.copy_from_slice(&bytes[..20]);
		Self(buffer)
	}
}

impl From<AccountId32> for AccountId20 {
	fn from(account: AccountId32) -> Self {
		let bytes: &[u8; 32] = account.as_ref();
		Self::from(*bytes)
	}
}

impl From<AccountId20> for Location {
	fn from(id: AccountId20) -> Self {
		Junction::AccountKey20 {
			network: None,
			key: id.into(),
		}
		.into()
	}
}

#[derive(Clone, Eq, PartialEq)]
#[derive(
	RuntimeDebug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EthereumSignature(ecdsa::Signature);

impl sp_runtime::traits::Verify for EthereumSignature {
	type Signer = EthereumSigner;
	fn verify<L: sp_runtime::traits::Lazy<[u8]>>(&self, mut msg: L, signer: &AccountId20) -> bool {
		let m = keccak_256(msg.get());
		match sp_io::crypto::secp256k1_ecdsa_recover(self.0.as_ref(), &m) {
			Ok(pubkey) => AccountId20(H160::from(H256::from(keccak_256(&pubkey))).0) == *signer,
			Err(sp_io::EcdsaVerifyError::BadRS) => {
				log::error!(target: "evm", "Error recovering: Incorrect value of R or S");
				false
			}
			Err(sp_io::EcdsaVerifyError::BadV) => {
				log::error!(target: "evm", "Error recovering: Incorrect value of V");
				false
			}
			Err(sp_io::EcdsaVerifyError::BadSignature) => {
				log::error!(target: "evm", "Error recovering: Invalid signature");
				false
			}
		}
	}
}

impl From<MultiSignature> for EthereumSignature {
	fn from(signature: MultiSignature) -> Self {
		match signature {
			MultiSignature::Ed25519(_) => {
				panic!("Ed25519 not supported for EthereumSignature")
			}
			MultiSignature::Sr25519(_) => {
				panic!("Sr25519 not supported for EthereumSignature")
			}
			MultiSignature::Ecdsa(sig) => Self(sig),
		}
	}
}

impl EthereumSignature {
	pub fn new(s: ecdsa::Signature) -> Self {
		Self(s)
	}
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[derive(RuntimeDebug, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[repr(transparent)]
pub struct EthereumSigner([u8; 20]);

impl From<[u8; 20]> for EthereumSigner {
	fn from(x: [u8; 20]) -> Self {
		EthereumSigner(x)
	}
}

impl sp_runtime::traits::IdentifyAccount for EthereumSigner {
	type AccountId = AccountId20;
	fn into_account(self) -> AccountId20 {
		AccountId20(self.0)
	}
}

impl fmt::Display for EthereumSigner {
	fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
		write!(fmt, "{:?}", H160::from(self.0))
	}
}

impl From<ecdsa::Public> for EthereumSigner {
	fn from(pk: ecdsa::Public) -> Self {
		let decompressed = libsecp256k1::PublicKey::parse_compressed(&pk.0)
			.expect("Wrong compressed public key provided")
			.serialize();
		let mut m = [0u8; 64];
		m.copy_from_slice(&decompressed[1..65]);
		let account = H160::from(H256::from(keccak_256(&m)));
		EthereumSigner(account.into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{ecdsa, Pair, H256};
	use sp_runtime::traits::IdentifyAccount;

	#[test]
	fn test_derive_from_secret_key() {
		let sk = hex::decode("eb3d6b0b0c794f6fd8964b4a28df99d4baa5f9c8d33603c4cc62504daa259358")
			.unwrap();
		let hex_acc: [u8; 20] = hex::decode("98fa2838ee6471ae87135880f870a785318e6787")
			.unwrap()
			.try_into()
			.unwrap();
		let acc = AccountId20::from(hex_acc);

		let pk = ecdsa::Pair::from_seed_slice(&sk).unwrap().public();
		let signer: EthereumSigner = pk.into();

		assert_eq!(signer.into_account(), acc);
	}

	#[test]
	fn test_from_h160() {
		let m = hex::decode("28490327ff4e60d44b8aadf5478266422ed01232cc712c2d617e5c650ca15b85")
			.unwrap();
		let old: AccountId20 = H160::from(H256::from(keccak_256(&m))).into();
		let new: AccountId20 = H160::from_slice(&keccak_256(&m)[12..32]).into();
		assert_eq!(new, old);
	}

	#[test]
	fn test_account_display() {
		let pk = ecdsa::Pair::from_string("//Alice", None)
			.expect("static values are valid; qed")
			.public();
		let signer: EthereumSigner = pk.into();
		let account: AccountId20 = signer.into_account();
		let account_fmt = format!("{}", account);
		assert_eq!(account_fmt, "0xE04CC55ebEE1cBCE552f250e85c57B70B2E2625b");
	}
}
