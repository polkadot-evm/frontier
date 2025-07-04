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
#![allow(clippy::too_many_arguments)]
#![warn(unused_crate_dependencies)]

extern crate alloc;

use alloc::vec::Vec;
use ethereum::Log;
use ethereum_types::{Address, Bloom};
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
// Substrate
use sp_core::{H256, U256};
use sp_runtime::{
	traits::{Block as BlockT, HashingFor},
	Permill, RuntimeDebug,
};
use sp_state_machine::OverlayedChanges;

#[derive(Clone, Eq, PartialEq, Default, RuntimeDebug, Encode, Decode, TypeInfo)]
pub struct TransactionStatus {
	pub transaction_hash: H256,
	pub transaction_index: u32,
	pub from: Address,
	pub to: Option<Address>,
	pub contract_address: Option<Address>,
	pub logs: Vec<Log>,
	pub logs_bloom: Bloom,
}

pub trait RuntimeStorageOverride<B: BlockT, C>: Send + Sync {
	fn is_enabled() -> bool;

	fn set_overlayed_changes(
		client: &C,
		overlayed_changes: &mut OverlayedChanges<HashingFor<B>>,
		block: B::Hash,
		version: u32,
		address: Address,
		balance: Option<U256>,
		nonce: Option<U256>,
	);

	fn into_account_id_bytes(address: Address) -> Vec<u8>;
}

impl<B: BlockT, C> RuntimeStorageOverride<B, C> for () {
	fn is_enabled() -> bool {
		false
	}

	fn set_overlayed_changes(
		_client: &C,
		_overlayed_changes: &mut OverlayedChanges<HashingFor<B>>,
		_block: B::Hash,
		_version: u32,
		_address: Address,
		_balance: Option<U256>,
		_nonce: Option<U256>,
	) {
	}

	fn into_account_id_bytes(_address: Address) -> Vec<u8> {
		Vec::default()
	}
}

sp_api::decl_runtime_apis! {
	/// API necessary for Ethereum-compatibility layer.
	#[api_version(6)]
	pub trait EthereumRuntimeRPCApi {
		/// Returns runtime defined pallet_evm::ChainId.
		fn chain_id() -> u64;

		/// Returns pallet_evm::Accounts by address.
		fn account_basic(address: Address) -> fp_evm::Account;

		/// Returns FixedGasPrice::min_gas_price
		fn gas_price() -> U256;

		/// For a given account address, returns pallet_evm::AccountCodes.
		fn account_code_at(address: Address) -> Vec<u8>;

		/// Returns the converted FindAuthor::find_author authority id.
		fn author() -> Address;

		/// For a given account address and index, returns pallet_evm::AccountStorages.
		fn storage_at(address: Address, index: U256) -> H256;

		/// Returns a frame_ethereum::call response. If `estimate` is true,
		#[changed_in(2)]
		fn call(
			from: Address,
			to: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<Vec<u8>>, sp_runtime::DispatchError>;
		#[changed_in(4)]
		fn call(
			from: Address,
			to: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<Vec<u8>>, sp_runtime::DispatchError>;
		#[changed_in(5)]
		fn call(
			from: Address,
			to: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(Address, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfo::<Vec<u8>>, sp_runtime::DispatchError>;
		#[changed_in(6)]
		fn call(
			from: Address,
			to: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(Address, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfoV2::<Vec<u8>>, sp_runtime::DispatchError>;
		#[allow(clippy::type_complexity)]
		fn call(
			from: Address,
			to: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(Address, Vec<H256>)>>,
			authorization_list: Option<Vec<(U256, Address, U256, Option<Address>)>>,
		) -> Result<fp_evm::ExecutionInfoV2::<Vec<u8>>, sp_runtime::DispatchError>;

		/// Returns a frame_ethereum::create response.
		#[changed_in(2)]
		fn create(
			from: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<Address>, sp_runtime::DispatchError>;
		#[changed_in(4)]
		fn create(
			from: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<Address>, sp_runtime::DispatchError>;
		#[changed_in(5)]
		fn create(
			from: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(Address, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfo::<Address>, sp_runtime::DispatchError>;
		#[changed_in(6)]
		fn create(
			from: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(Address, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfoV2::<Address>, sp_runtime::DispatchError>;
		#[allow(clippy::type_complexity)]
		fn create(
			from: Address,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(Address, Vec<H256>)>>,
			authorization_list: Option<Vec<(U256, Address, U256, Option<Address>)>>,
		) -> Result<fp_evm::ExecutionInfoV2::<Address>, sp_runtime::DispatchError>;

		/// Return the current block. Legacy.
		#[changed_in(2)]
		fn current_block() -> Option<ethereum::BlockV0>;
		/// Return the current block.
		fn current_block() -> Option<ethereum::BlockV3>;

		/// Return the current receipt.
		#[changed_in(4)]
		fn current_receipts() -> Option<Vec<ethereum::ReceiptV0>>;
		/// Return the current receipt.
		fn current_receipts() -> Option<Vec<ethereum::ReceiptV3>>;

		/// Return the current transaction status.
		fn current_transaction_statuses() -> Option<Vec<TransactionStatus>>;

		/// Return all the current data for a block in a single runtime call. Legacy.
		#[changed_in(2)]
		fn current_all() -> (
			Option<ethereum::BlockV0>,
			Option<Vec<ethereum::ReceiptV0>>,
			Option<Vec<TransactionStatus>>
		);
		/// Return all the current data for a block in a single runtime call.
		#[changed_in(4)]
		fn current_all() -> (
			Option<ethereum::BlockV3>,
			Option<Vec<ethereum::ReceiptV0>>,
			Option<Vec<TransactionStatus>>
		);
		fn current_all() -> (
			Option<ethereum::BlockV3>,
			Option<Vec<ethereum::ReceiptV3>>,
			Option<Vec<TransactionStatus>>
		);

		/// Receives a `Vec<OpaqueExtrinsic>` and filters all the ethereum transactions. Legacy.
		#[changed_in(2)]
		fn extrinsic_filter(
			xts: Vec<<Block as BlockT>::Extrinsic>,
		) -> Vec<ethereum::TransactionV0>;
		/// Receives a `Vec<OpaqueExtrinsic>` and filters all the ethereum transactions.
		fn extrinsic_filter(
			xts: Vec<<Block as BlockT>::Extrinsic>,
		) -> Vec<ethereum::TransactionV3>;

		/// Return the elasticity multiplier.
		fn elasticity() -> Option<Permill>;

		/// Used to determine if gas limit multiplier for non-transactional calls (eth_call/estimateGas)
		/// is supported.
		fn gas_limit_multiplier_support();

		/// Return the pending block.
		fn pending_block(
			xts: Vec<<Block as BlockT>::Extrinsic>,
		) -> (Option<ethereum::BlockV3>, Option<Vec<TransactionStatus>>);
		/// Initialize the pending block.
		/// The behavior should be the same as the runtime api Core_initialize_block but
		/// for a "pending" block.
		/// If your project don't need to have a different behavior to initialize "pending" blocks,
		/// you can copy your Core_initialize_block implementation.
		fn initialize_pending_block(header: &<Block as BlockT>::Header);
	}

	#[api_version(2)]
	pub trait ConvertTransactionRuntimeApi {
		fn convert_transaction(transaction: ethereum::TransactionV3) -> <Block as BlockT>::Extrinsic;
		#[changed_in(2)]
		fn convert_transaction(transaction: ethereum::TransactionV0) -> <Block as BlockT>::Extrinsic;
	}
}

/// Fallback transaction converter when the `ConvertTransactionRuntimeApi` is not available. For almost all
/// non-legacy cases, you can instantiate this type as `NoTransactionConverter`.
pub trait ConvertTransaction<E> {
	fn convert_transaction(&self, transaction: ethereum::TransactionV3) -> E;
}

/// No fallback transaction converter is available.
// `NoTransactionConverter` is a non-instantiable type (an enum with no variants),
// so we are guaranteed at compile time that `NoTransactionConverter` can never be instantiated.
pub enum NoTransactionConverter {}
impl<E> ConvertTransaction<E> for NoTransactionConverter {
	// `convert_transaction` is a method taking `&self` as a parameter, so it can only be called via an instance of type Self,
	// so we are guaranteed at compile time that this method can never be called.
	fn convert_transaction(&self, _transaction: ethereum::TransactionV3) -> E {
		match *self {}
	}
}
