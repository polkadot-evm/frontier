// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
//
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

//! Precompile implementations.

use sp_std::mem::size_of;
use sp_std::cmp::max;
use sp_std::{cmp::min, vec::Vec};
use sp_core::H160;
use evm::{ExitError, ExitSucceed};
use ripemd160::Digest;
use impl_trait_for_tuples::impl_for_tuples;
use sp_std::convert::TryFrom;

#[cfg(feature = "ed25519")]
use ed25519_dalek::{PublicKey, Verifier, Signature};

#[cfg(feature = "modexp")]
use num::{BigUint, Zero, One};

#[cfg(feature = "blake2")]
use blake2_rfc::blake2b::Blake2b;

#[cfg(feature = "bn128")]
use ethereum_types::{U256};

/// Custom precompiles to be used by EVM engine.
pub trait Precompiles {
	/// Try to execute the code address as precompile. If the code address is not
	/// a precompile or the precompile is not yet available, return `None`.
	/// Otherwise, calculate the amount of gas needed with given `input` and
	/// `target_gas`. Return `Some(Ok(status, output, gas_used))` if the execution
	/// is successful. Otherwise return `Some(Err(_))`.
	fn execute(
		address: H160,
		input: &[u8],
		target_gas: Option<usize>,
	) -> Option<core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError>>;
}

/// One single precompile used by EVM engine.
pub trait Precompile {
	/// Try to execute the precompile. Calculate the amount of gas needed with given `input` and
	/// `target_gas`. Return `Ok(status, output, gas_used)` if the execution is
	/// successful. Otherwise return `Err(_)`.
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError>;
}

#[impl_for_tuples(16)]
#[tuple_types_no_default_trait_bound]
impl Precompiles for Tuple {
	for_tuples!( where #( Tuple: Precompile )* );

	fn execute(
		address: H160,
		input: &[u8],
		target_gas: Option<usize>,
	) -> Option<core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError>> {
		let mut index = 0;

		for_tuples!( #(
			index += 1;
			if address == H160::from_low_u64_be(index) {
				return Some(Tuple::execute(input, target_gas))
			}
		)* );

		None
	}
}

/// Linear gas cost
fn ensure_linear_cost(
	target_gas: Option<usize>,
	len: usize,
	base: usize,
	word: usize
) -> Result<usize, ExitError> {
	let cost = base.checked_add(
		word.checked_mul(len.saturating_add(31) / 32).ok_or(ExitError::OutOfGas)?
	).ok_or(ExitError::OutOfGas)?;

	if let Some(target_gas) = target_gas {
		if cost > target_gas {
			return Err(ExitError::OutOfGas)
		}
	}

	Ok(cost)
}

/// The identity precompile.
pub struct Identity;

impl Precompile for Identity {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, input.len(), 15, 3)?;

		Ok((ExitSucceed::Returned, input.to_vec(), cost))
	}
}

/// The ecrecover precompile.
pub struct ECRecover;

impl Precompile for ECRecover {
	fn execute(
		i: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, i.len(), 3000, 0)?;

		let mut input = [0u8; 128];
		input[..min(i.len(), 128)].copy_from_slice(&i[..min(i.len(), 128)]);

		let mut msg = [0u8; 32];
		let mut sig = [0u8; 65];

		msg[0..32].copy_from_slice(&input[0..32]);
		sig[0..32].copy_from_slice(&input[64..96]);
		sig[32..64].copy_from_slice(&input[96..128]);
		sig[64] = input[63];

		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg)
			.map_err(|_| ExitError::Other("Public key recover failed".into()))?;
		let mut address = sp_io::hashing::keccak_256(&pubkey);
		address[0..12].copy_from_slice(&[0u8; 12]);

		Ok((ExitSucceed::Returned, address.to_vec(), cost))
	}
}

/// The ripemd precompile.
pub struct Ripemd160;

impl Precompile for Ripemd160 {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, input.len(), 600, 120)?;

		let mut ret = [0u8; 32];
		ret[12..32].copy_from_slice(&ripemd160::Ripemd160::digest(input));
		Ok((ExitSucceed::Returned, ret.to_vec(), cost))
	}
}

/// The sha256 precompile.
pub struct Sha256;

impl Precompile for Sha256 {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, input.len(), 60, 12)?;

		let ret = sp_io::hashing::sha2_256(input);
		Ok((ExitSucceed::Returned, ret.to_vec(), cost))
	}
}

#[cfg(feature = "modexp")]
pub struct Modexp;

#[cfg(feature = "modexp")]
impl Precompile for Modexp {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, input.len(), 15, 3)?;
		let mut buf = [0; 32];
		buf.copy_from_slice(&input[0..32]);
		let mut len_bytes = [0u8; 8];
		len_bytes.copy_from_slice(&buf[24..]);
		let base_len = u64::from_be_bytes(len_bytes) as usize;

		buf = [0; 32];
		buf.copy_from_slice(&input[32..64]);
		len_bytes = [0u8; 8];
		len_bytes.copy_from_slice(&buf[24..]);
		let exp_len = u64::from_be_bytes(len_bytes) as usize;

		buf = [0; 32];
		buf.copy_from_slice(&input[64..96]);
		len_bytes = [0u8; 8];
		len_bytes.copy_from_slice(&buf[24..]);
		let mod_len = u64::from_be_bytes(len_bytes) as usize;

		// Gas formula allows arbitrary large exp_len when base and modulus are empty, so we need to handle empty base first.
		let r = if base_len == 0 && mod_len == 0 {
			BigUint::zero()
		} else {
			// read the numbers themselves.
			let mut buf = Vec::with_capacity(max(mod_len, max(base_len, exp_len)));
			buf.copy_from_slice(&input[0..base_len]);
			let base = BigUint::from_bytes_be(&buf[..base_len]);

			buf = Vec::with_capacity(max(mod_len, max(base_len, exp_len)));
			buf.copy_from_slice(&input[base_len..base_len + exp_len]);
			let exponent = BigUint::from_bytes_be(&buf[..exp_len]);

			buf = Vec::with_capacity(max(mod_len, max(base_len, exp_len)));
			buf.copy_from_slice(&input[(base_len + exp_len)..(base_len + exp_len + mod_len)]);
			let modulus = BigUint::from_bytes_be(&buf[..mod_len]);

			if modulus.is_zero() || modulus.is_one() {
				BigUint::zero()
			} else {
				base.modpow(&exponent, &modulus)
			}
		};

		// write output to given memory, left padded and same length as the modulus.
		let bytes = r.to_bytes_be();

		// always true except in the case of zero-length modulus, which leads to
		// output of length and value 1.
		if bytes.len() <= mod_len {
			let res_start = mod_len - bytes.len();
			let mut ret = Vec::with_capacity(bytes.len() - mod_len);
			ret.copy_from_slice(&bytes[res_start..bytes.len()]);
			Ok((ExitSucceed::Returned, ret.to_vec(), cost))
		} else {
			Err(ExitError::Other("failed".into()))
		}
	}
}

#[cfg(feature = "ed25519")]
pub struct Ed25519Verify;

#[cfg(feature = "ed25519")]
impl Precompile for Ed25519Verify {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, input.len(), 15, 3)?;

		let len = min(input.len(), 128);

		let mut i = [0u8; 128];
		i[..len].copy_from_slice(&input[..len]);

		let mut buf = [0u8; 4];

		let msg = &i[0..32];
		let pk = PublicKey::from_bytes(&i[32..64])
			.map_err(|_| ExitError::Other("Public key recover failed".into()))?;
		let sig = Signature::try_from(&i[64..128])
			.map_err(|_| ExitError::Other("Signature recover failed".into()))?;

		// https://docs.rs/rust-crypto/0.2.36/crypto/ed25519/fn.verify.html
		if pk.verify(msg, &sig).is_ok() {
			buf[3] = 0u8;
		} else {
			buf[3] = 1u8;
		};

		Ok((ExitSucceed::Returned, buf.to_vec(), cost))
	}
}

#[cfg(feature = "bn128")]
fn read_fr(input: &[u8], start_inx: usize) -> Result<bn::Fr, ExitError> {
	bn::Fr::from_slice(&input[start_inx..(start_inx + 32)]).map_err(|_| ExitError::Other("Invalid field element".into()))
}

#[cfg(feature = "bn128")]
fn read_point(input: &[u8], start_inx: usize) -> Result<bn::G1, ExitError> {
	use bn::{Fq, AffineG1, G1, Group};

	let px = Fq::from_slice(&input[start_inx..(start_inx + 32)]).map_err(|_| ExitError::Other("Invalid point x coordinate".into()))?;
	let py = Fq::from_slice(&input[(start_inx + 32)..(start_inx + 64)]).map_err(|_| ExitError::Other("Invalid point y coordinate".into()))?;
	Ok(
		if px == Fq::zero() && py == Fq::zero() {
			G1::zero()
		} else {
			AffineG1::new(px, py).map_err(|_| ExitError::Other("Invalid curve point".into()))?.into()
		}
	)
}

/// The Bn128Add builtin
#[cfg(feature = "bn128")]
pub struct Bn128Add;

/// The Bn128Mul builtin
#[cfg(feature = "bn128")]
pub struct Bn128Mul;

/// The Bn128Pairing builtin
#[cfg(feature = "bn128")]
pub struct Bn128Pairing;

/// The bn128 addition arithmetic precompile.
#[cfg(feature = "bn128")]
impl Precompile for Bn128Add {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, input.len(), 15, 3)?;

		use bn::AffineG1;

		let p1 = read_point(input, 0)?;
		let p2 = read_point(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p1 + p2) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).map_err(|_| ExitError::Other("Cannot fail since 0..32 is 32-byte length".into()))?;
			sum.y().to_big_endian(&mut buf[32..64]).map_err(|_| ExitError::Other("Cannot fail since 32..64 is 32-byte length".into()))?;
		}

		Ok((ExitSucceed::Returned, buf.to_vec(), cost))
	}
}

/// The bn128 multiplication arithmetic precompile.
#[cfg(feature = "bn128")]
impl Precompile for Bn128Mul {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		use bn::AffineG1;

		let cost = ensure_linear_cost(target_gas, input.len(), 15, 3)?;

		let p = read_point(input, 0)?;
		let fr = read_fr(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p * fr) {
			// point not at infinity
			sum.x().to_big_endian(&mut buf[0..32]).map_err(|_| ExitError::Other("Cannot fail since 0..32 is 32-byte length".into()))?;
			sum.y().to_big_endian(&mut buf[32..64]).map_err(|_| ExitError::Other("Cannot fail since 32..64 is 32-byte length".into()))?;
		}

		Ok((ExitSucceed::Returned, buf.to_vec(), cost))
	}
}

/// The bn128 pairing precompile.
#[cfg(feature = "bn128")]
impl Precompile for Bn128Pairing {
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		use bn::{AffineG1, AffineG2, Fq, Fq2, pairing_batch, G1, G2, Gt, Group};

		let cost = ensure_linear_cost(target_gas, input.len(), 15, 3)?;

		let ret_val = if input.is_empty() {
			U256::one()
		} else {
			// (a, b_a, b_b - each 64-byte affine coordinates)
			let elements = input.len() / 192;
			let mut vals = Vec::new();
			for idx in 0..elements {
				let a_x = Fq::from_slice(&input[idx*192..idx*192+32])
					.map_err(|_| ExitError::Other("Invalid a argument x coordinate".into()))?;

				let a_y = Fq::from_slice(&input[idx*192+32..idx*192+64])
					.map_err(|_| ExitError::Other("Invalid a argument y coordinate".into()))?;

				let b_a_y = Fq::from_slice(&input[idx*192+64..idx*192+96])
					.map_err(|_| ExitError::Other("Invalid b argument imaginary coeff x coordinate".into()))?;

				let b_a_x = Fq::from_slice(&input[idx*192+96..idx*192+128])
					.map_err(|_| ExitError::Other("Invalid b argument imaginary coeff y coordinate".into()))?;

				let b_b_y = Fq::from_slice(&input[idx*192+128..idx*192+160])
					.map_err(|_| ExitError::Other("Invalid b argument real coeff x coordinate".into()))?;

				let b_b_x = Fq::from_slice(&input[idx*192+160..idx*192+192])
					.map_err(|_| ExitError::Other("Invalid b argument real coeff y coordinate".into()))?;

				let b_a = Fq2::new(b_a_x, b_a_y);
				let b_b = Fq2::new(b_b_x, b_b_y);
				let b = if b_a.is_zero() && b_b.is_zero() {
					G2::zero()
				} else {
					G2::from(AffineG2::new(b_a, b_b).map_err(|_| ExitError::Other("Invalid b argument - not on curve".into()))?)
				};
				let a = if a_x.is_zero() && a_y.is_zero() {
					G1::zero()
				} else {
					G1::from(AffineG1::new(a_x, a_y).map_err(|_| ExitError::Other("Invalid a argument - not on curve".into()))?)
				};
				vals.push((a, b));
			};

			let mul = pairing_batch(&vals);

			if mul == Gt::one() {
				U256::one()
			} else {
				U256::zero()
			}
		};

		let mut buf = [0u8; 32];
		ret_val.to_big_endian(&mut buf);

		Ok((ExitSucceed::Returned, buf.to_vec(), cost))
	}
}

#[cfg(feature = "blake2f")]
pub struct Blake2F;

#[cfg(feature = "blake2f")]
impl Precompile for Blake2F {
	/// Format of `input`:
	/// [4 bytes for rounds][64 bytes for h][128 bytes for m][8 bytes for t_0][8 bytes for t_1][1 byte for f]
	fn execute(
		input: &[u8],
		target_gas: Option<usize>,
	) -> core::result::Result<(ExitSucceed, Vec<u8>, usize), ExitError> {
		let cost = ensure_linear_cost(target_gas, input.len(), 15, 3)?;
		const BLAKE2_F_ARG_LEN: usize = 213;

		if input.len() != BLAKE2_F_ARG_LEN {
			return Err(ExitError::Other("input length for Blake2 F precompile should be exactly 213 bytes".into()));
		}

		let mut rounds_buf: [u8; 4] = [0; 4];
		rounds_buf.copy_from_slice(&input[0..4]);
		let rounds: u32 = u32::from_le_bytes(rounds_buf);

		let mut h_buf: [u8; 64] = [0; 64];
		h_buf.copy_from_slice(&input[4..48]);
		let mut h = [0u64; 8];
		let mut ctr = 0;
		for state_word in &mut h {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&h_buf[(ctr + 8)..(ctr + 1) * 8]);
			*state_word = u64::from_le_bytes(temp).into();
			ctr += 1;
		}

		let mut m_buf: [u8; 128] = [0; 128];
		m_buf.copy_from_slice(&input[68..196]);
		let mut m = [0u64; 16];
		ctr = 0;
		for msg_word in &mut m {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&m_buf[(ctr + 8)..(ctr + 1) * 8]);
			*msg_word = u64::from_le_bytes(temp).into();
			ctr += 1;
		}


		let mut t_0_buf: [u8; 8] = [0; 8];
		t_0_buf.copy_from_slice(&input[196..204]);
		let t_0 = u64::from_le_bytes(t_0_buf);

		let mut t_1_buf: [u8; 8] = [0; 8];
		t_1_buf.copy_from_slice(&input[204..212]);
		let t_1 = u64::from_le_bytes(t_1_buf);

		let f = if input[212] == 1 { true } else if input[212] == 0 { false } else {
			return Err(ExitError::Other("incorrect final block indicator flag".into()))
		};

		crate::eip_152::compress(&mut h, m, [t_0.into(), t_1.into()], f, rounds as usize);

		let mut output_buf = [0u8; 8 * size_of::<u64>()];
		for (i, state_word) in h.iter().enumerate() {
			output_buf[i * 8..(i + 1) * 8].copy_from_slice(&state_word.to_le_bytes());
		}

		Ok((ExitSucceed::Returned, output_buf.to_vec(), cost))
	}
}
