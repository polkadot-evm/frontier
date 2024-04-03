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

use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

use ethereum_types::{H160, H256, U256};
use evm::{ExitError, ExitReason};
use jsonrpsee::{core::RpcResult, types::error::CALL_EXECUTION_FAILED_CODE};
use scale_codec::{Decode, Encode};
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool::ChainApi;
use sp_api::{ApiExt, CallApiAt, CallApiAtParams, CallContext, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_externalities::Extensions;
use sp_inherents::CreateInherentDataProviders;
use sp_io::hashing::{blake2_128, twox_128};
use sp_runtime::{
	traits::{Block as BlockT, HashingFor},
	DispatchError, SaturatedConversion,
};
use sp_state_machine::OverlayedChanges;
// Frontier
use fc_rpc_core::types::*;
use fp_evm::{ExecutionInfo, ExecutionInfoV2};
use fp_rpc::{EthereumRuntimeRPCApi, RuntimeStorageOverride};
use fp_storage::{EVM_ACCOUNT_CODES, PALLET_EVM};

use crate::{
	eth::{Eth, EthConfig},
	frontier_backend_client, internal_err,
};

/// Allow to adapt a request for `estimate_gas`.
/// Can be used to estimate gas of some contracts using a different function
/// in the case the normal gas estimation doesn't work.
///
/// Example: a precompile that tries to do a subcall but succeeds regardless of the
/// success of the subcall. The gas estimation will thus optimize the gas limit down
/// to the minimum, while we want to estimate a gas limit that will allow the subcall to
/// have enough gas to succeed.
pub trait EstimateGasAdapter {
	fn adapt_request(request: TransactionRequest) -> TransactionRequest;
}

impl EstimateGasAdapter for () {
	fn adapt_request(request: TransactionRequest) -> TransactionRequest {
		request
	}
}

impl<B, C, P, CT, BE, A, CIDP, EC> Eth<B, C, P, CT, BE, A, CIDP, EC>
where
	B: BlockT,
	C: CallApiAt<B> + ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	A: ChainApi<Block = B>,
	CIDP: CreateInherentDataProviders<B, ()> + Send + 'static,
	EC: EthConfig<B, C>,
{
	pub async fn call(
		&self,
		request: TransactionRequest,
		number_or_hash: Option<BlockNumberOrHash>,
		state_overrides: Option<BTreeMap<H160, CallStateOverride>>,
	) -> RpcResult<Bytes> {
		let TransactionRequest {
			from,
			to,
			gas_price,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			gas,
			value,
			data,
			nonce,
			access_list,
			..
		} = request;

		let (gas_price, max_fee_per_gas, max_priority_fee_per_gas) = {
			let details = fee_details(gas_price, max_fee_per_gas, max_priority_fee_per_gas)?;
			(
				details.gas_price,
				details.max_fee_per_gas,
				details.max_priority_fee_per_gas,
			)
		};

		let (substrate_hash, api) = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number_or_hash,
		)
		.await?
		{
			Some(id) => {
				let hash = self.client.expect_block_hash_from_id(&id).map_err(|_| {
					crate::err(CALL_EXECUTION_FAILED_CODE, "header not found", None)
				})?;
				(hash, self.client.runtime_api())
			}
			None => {
				// Not mapped in the db, assume pending.
				let (hash, api) = self.pending_runtime_api().await.map_err(|err| {
					internal_err(format!("Create pending runtime api error: {err}"))
				})?;
				(hash, api)
			}
		};

		let api_version = if let Ok(Some(api_version)) =
			api.api_version::<dyn EthereumRuntimeRPCApi<B>>(substrate_hash)
		{
			api_version
		} else {
			return Err(internal_err("failed to retrieve Runtime Api version"));
		};

		let block = if api_version > 1 {
			api.current_block(substrate_hash)
				.map_err(|err| internal_err(format!("runtime error: {err}")))?
		} else {
			#[allow(deprecated)]
			let legacy_block = api
				.current_block_before_version_2(substrate_hash)
				.map_err(|err| internal_err(format!("runtime error: {err}")))?;
			legacy_block.map(|block| block.into())
		};

		let block_gas_limit = block
			.ok_or_else(|| internal_err("block unavailable, cannot query gas limit"))?
			.header
			.gas_limit;
		let max_gas_limit = block_gas_limit * self.execute_gas_limit_multiplier;

		// use given gas limit or query current block's limit
		let gas_limit = match gas {
			Some(amount) => {
				if amount > max_gas_limit {
					return Err(internal_err(format!(
						"provided gas limit is too high (can be up to {}x the block gas limit)",
						self.execute_gas_limit_multiplier
					)));
				}
				amount
			}
			// If gas limit is not specified in the request we either use the multiplier if supported
			// or fallback to the block gas limit.
			None => match api.gas_limit_multiplier_support(substrate_hash) {
				Ok(_) => max_gas_limit,
				_ => block_gas_limit,
			},
		};

		let data = data.into_bytes().map(|d| d.into_vec()).unwrap_or_default();
		match to {
			Some(to) => {
				if api_version == 1 {
					// Legacy pre-london
					#[allow(deprecated)]
					let info = api.call_before_version_2(
						substrate_hash,
						from.unwrap_or_default(),
						to,
						data,
						value.unwrap_or_default(),
						gas_limit,
						gas_price,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {err}")))?
					.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else if api_version >= 2 && api_version < 4 {
					// Post-london
					#[allow(deprecated)]
					let info = api.call_before_version_4(
						substrate_hash,
						from.unwrap_or_default(),
						to,
						data,
						value.unwrap_or_default(),
						gas_limit,
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {err}")))?
					.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else if api_version == 4 || api_version == 5 {
					// Post-london + access list support
					let encoded_params = Encode::encode(&(
						&from.unwrap_or_default(),
						&to,
						&data,
						&value.unwrap_or_default(),
						&gas_limit,
						&max_fee_per_gas,
						&max_priority_fee_per_gas,
						&nonce,
						&false,
						&Some(
							access_list
								.unwrap_or_default()
								.into_iter()
								.map(|item| (item.address, item.storage_keys))
								.collect::<Vec<(sp_core::H160, Vec<H256>)>>(),
						),
					));
					let overlayed_changes = self.create_overrides_overlay(
						substrate_hash,
						api_version,
						state_overrides,
					)?;
					let params = CallApiAtParams {
						at: substrate_hash,
						function: "EthereumRuntimeRPCApi_call",
						arguments: encoded_params,
						overlayed_changes: &RefCell::new(overlayed_changes),
						call_context: CallContext::Offchain,
						recorder: &None,
						extensions: &RefCell::new(Extensions::new()),
					};

					let value = if api_version == 4 {
						let info = self
							.client
							.call_api_at(params)
							.and_then(|r| {
								Result::map_err(
									<Result<ExecutionInfo::<Vec<u8>>, DispatchError> as Decode>::decode(&mut &r[..]),
									|error| sp_api::ApiError::FailedToDecodeReturnValue {
										function: "EthereumRuntimeRPCApi_call",
										error,
										raw: r
									},
								)
							})
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

						error_on_execution_failure(&info.exit_reason, &info.value)?;
						info.value
					} else if api_version == 5 {
						let info = self
							.client
							.call_api_at(params)
							.and_then(|r| {
								Result::map_err(
									<Result<ExecutionInfoV2::<Vec<u8>>, DispatchError> as Decode>::decode(&mut &r[..]),
									|error| sp_api::ApiError::FailedToDecodeReturnValue {
										function: "EthereumRuntimeRPCApi_call",
										error,
										raw: r
									},
								)
							})
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

						error_on_execution_failure(&info.exit_reason, &info.value)?;
						info.value
					} else {
						unreachable!("invalid version");
					};

					Ok(Bytes(value))
				} else {
					Err(internal_err("failed to retrieve Runtime Api version"))
				}
			}
			None => {
				if api_version == 1 {
					// Legacy pre-london
					#[allow(deprecated)]
					let info = api.create_before_version_2(
						substrate_hash,
						from.unwrap_or_default(),
						data,
						value.unwrap_or_default(),
						gas_limit,
						gas_price,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {err}")))?
					.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(substrate_hash, info.value)
						.map_err(|err| internal_err(format!("runtime error: {err}")))?;
					Ok(Bytes(code))
				} else if api_version >= 2 && api_version < 4 {
					// Post-london
					#[allow(deprecated)]
					let info = api.create_before_version_4(
						substrate_hash,
						from.unwrap_or_default(),
						data,
						value.unwrap_or_default(),
						gas_limit,
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {err}")))?
					.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(substrate_hash, info.value)
						.map_err(|err| internal_err(format!("runtime error: {err}")))?;
					Ok(Bytes(code))
				} else if api_version == 4 {
					// Post-london + access list support
					let access_list = access_list.unwrap_or_default();
					#[allow(deprecated)]
					let info = api.create_before_version_5(
						substrate_hash,
						from.unwrap_or_default(),
						data,
						value.unwrap_or_default(),
						gas_limit,
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						false,
						Some(
							access_list
								.into_iter()
								.map(|item| (item.address, item.storage_keys))
								.collect(),
						),
					)
					.map_err(|err| internal_err(format!("runtime error: {err}")))?
					.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(substrate_hash, info.value)
						.map_err(|err| internal_err(format!("runtime error: {err}")))?;
					Ok(Bytes(code))
				} else if api_version == 5 {
					// Post-london + access list support
					let access_list = access_list.unwrap_or_default();
					let info = api
						.create(
							substrate_hash,
							from.unwrap_or_default(),
							data,
							value.unwrap_or_default(),
							gas_limit,
							max_fee_per_gas,
							max_priority_fee_per_gas,
							nonce,
							false,
							Some(
								access_list
									.into_iter()
									.map(|item| (item.address, item.storage_keys))
									.collect(),
							),
						)
						.map_err(|err| internal_err(format!("runtime error: {err}")))?
						.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(substrate_hash, info.value)
						.map_err(|err| internal_err(format!("runtime error: {err}")))?;
					Ok(Bytes(code))
				} else {
					Err(internal_err("failed to retrieve Runtime Api version"))
				}
			}
		}
	}

	pub async fn estimate_gas(
		&self,
		request: TransactionRequest,
		number_or_hash: Option<BlockNumberOrHash>,
	) -> RpcResult<U256> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);

		// Define the lower bound of estimate
		const MIN_GAS_PER_TX: U256 = U256([21_000, 0, 0, 0]);

		// Get substrate hash and runtime api
		let (substrate_hash, api) = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number_or_hash,
		)
		.await?
		{
			Some(id) => {
				let hash = client.expect_block_hash_from_id(&id).map_err(|_| {
					crate::err(CALL_EXECUTION_FAILED_CODE, "header not found", None)
				})?;
				(hash, client.runtime_api())
			}
			None => {
				// Not mapped in the db, assume pending.
				let (hash, api) = self.pending_runtime_api().await.map_err(|err| {
					internal_err(format!("Create pending runtime api error: {err}"))
				})?;
				(hash, api)
			}
		};

		// Adapt request for gas estimation.
		let request = EC::EstimateGasAdapter::adapt_request(request);

		// For simple transfer to simple account, return MIN_GAS_PER_TX directly
		let is_simple_transfer = match &request.data() {
			None => true,
			Some(vec) => vec.0.is_empty(),
		};
		if is_simple_transfer {
			if let Some(to) = request.to {
				let to_code = api
					.account_code_at(substrate_hash, to)
					.map_err(|err| internal_err(format!("runtime error: {err}")))?;
				if to_code.is_empty() {
					return Ok(MIN_GAS_PER_TX);
				}
			}
		}

		let block_gas_limit = {
			let schema = fc_storage::onchain_storage_schema(client.as_ref(), substrate_hash);
			let block = block_data_cache.current_block(schema, substrate_hash).await;
			block
				.ok_or_else(|| internal_err("block unavailable, cannot query gas limit"))?
				.header
				.gas_limit
		};

		let max_gas_limit = block_gas_limit * self.execute_gas_limit_multiplier;

		// Determine the highest possible gas limits
		let mut highest = match request.gas {
			Some(amount) => {
				if amount > max_gas_limit {
					return Err(internal_err(format!(
						"provided gas limit is too high (can be up to {}x the block gas limit)",
						self.execute_gas_limit_multiplier
					)));
				}
				amount
			}
			// If gas limit is not specified in the request we either use the multiplier if supported
			// or fallback to the block gas limit.
			None => match api.gas_limit_multiplier_support(substrate_hash) {
				Ok(_) => max_gas_limit,
				_ => block_gas_limit,
			},
		};

		let (gas_price, max_fee_per_gas, max_priority_fee_per_gas, fee_cap) = {
			let details = fee_details(
				request.gas_price,
				request.max_fee_per_gas,
				request.max_priority_fee_per_gas,
			)?;
			(
				details.gas_price,
				details.max_fee_per_gas,
				details.max_priority_fee_per_gas,
				details.fee_cap,
			)
		};

		// Recap the highest gas allowance with account's balance.
		if let Some(from) = request.from {
			if fee_cap > U256::zero() {
				let balance = api
					.account_basic(substrate_hash, from)
					.map_err(|err| internal_err(format!("runtime error: {err}")))?
					.balance;
				let mut available = balance;
				if let Some(value) = request.value {
					if value > available {
						return Err(internal_err("insufficient funds for transfer"));
					}
					available -= value;
				}
				let allowance = available / fee_cap;
				if highest > allowance {
					log::warn!(
							"Gas estimation capped by limited funds original {} balance {} sent {} feecap {} fundable {}",
							highest,
							balance,
							request.value.unwrap_or_default(),
							fee_cap,
							allowance
						);
					highest = allowance;
				}
			}
		}

		struct ExecutableResult {
			data: Vec<u8>,
			exit_reason: ExitReason,
			used_gas: U256,
		}

		// Create a helper to check if a gas allowance results in an executable transaction.
		//
		// A new ApiRef instance needs to be used per execution to avoid the overlayed state to affect
		// the estimation result of subsequent calls.
		//
		// Note that this would have a performance penalty if we introduce gas estimation for past
		// blocks - and thus, past runtime versions. Substrate has a default `runtime_cache_size` of
		// 2 slots LRU-style, meaning if users were to access multiple runtime versions in a short period
		// of time, the RPC response time would degrade a lot, as the VersionedRuntime needs to be compiled.
		//
		// To solve that, and if we introduce historical gas estimation, we'd need to increase that default.
		#[rustfmt::skip]
			let executable = move |
				request, gas_limit, api_version, api: sp_api::ApiRef<'_, C::Api>, estimate_mode
			| -> RpcResult<ExecutableResult> {
				let TransactionRequest {
					from,
					to,
					gas,
					value,
					data,
					nonce,
					access_list,
					..
				} = request;

				// Use request gas limit only if it less than gas_limit parameter
				let gas_limit = core::cmp::min(gas.unwrap_or(gas_limit), gas_limit);

				let data = data.into_bytes().map(|d| d.0).unwrap_or_default();

				let (exit_reason, data, used_gas) = match to {
					Some(to) => {
						if api_version == 1 {
							// Legacy pre-london
							#[allow(deprecated)]
							let info = api.call_before_version_2(
								substrate_hash,
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								gas_price,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, info.value, info.used_gas)
						} else if api_version < 4 {
							// Post-london
							#[allow(deprecated)]
							let info = api.call_before_version_4(
								substrate_hash,
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, info.value, info.used_gas)
						} else if api_version == 4 {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							#[allow(deprecated)]
							let info = api.call_before_version_5(
								substrate_hash,
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
								Some(
									access_list
										.into_iter()
										.map(|item| (item.address, item.storage_keys))
										.collect(),
								),
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, info.value, info.used_gas)
						} else {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							let info = api.call(
								substrate_hash,
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
								Some(
									access_list
										.into_iter()
										.map(|item| (item.address, item.storage_keys))
										.collect(),
								),
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, info.value, info.used_gas.effective)
						}
					}
					None => {
						if api_version == 1 {
							// Legacy pre-london
							#[allow(deprecated)]
							let info = api.create_before_version_2(
								substrate_hash,
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								gas_price,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, Vec::new(), info.used_gas)
						} else if api_version < 4 {
							// Post-london
							#[allow(deprecated)]
							let info = api.create_before_version_4(
								substrate_hash,
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, Vec::new(), info.used_gas)
						} else if api_version == 4 {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							#[allow(deprecated)]
							let info = api.create_before_version_5(
								substrate_hash,
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
								Some(
									access_list
										.into_iter()
										.map(|item| (item.address, item.storage_keys))
										.collect(),
								),
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, Vec::new(), info.used_gas)
						} else {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							let info = api.create(
								substrate_hash,
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
								Some(
									access_list
										.into_iter()
										.map(|item| (item.address, item.storage_keys))
										.collect(),
								),
							)
							.map_err(|err| internal_err(format!("runtime error: {err}")))?
							.map_err(|err| internal_err(format!("execution fatal: {err:?}")))?;

							(info.exit_reason, Vec::new(), info.used_gas.effective)
						}
					}
				};
				Ok(ExecutableResult {
					exit_reason,
					data,
					used_gas,
				})
			};
		let api_version = if let Ok(Some(api_version)) =
			client
				.runtime_api()
				.api_version::<dyn EthereumRuntimeRPCApi<B>>(substrate_hash)
		{
			api_version
		} else {
			return Err(internal_err("failed to retrieve Runtime Api version"));
		};

		// Verify that the transaction succeed with the highest capacity
		let cap = highest;
		let estimate_mode = !cfg!(feature = "rpc-binary-search-estimate");
		let ExecutableResult {
			data,
			exit_reason,
			used_gas,
		} = executable(
			request.clone(),
			highest,
			api_version,
			client.runtime_api(),
			estimate_mode,
		)?;
		match exit_reason {
			ExitReason::Succeed(_) => (),
			ExitReason::Error(ExitError::OutOfGas) => {
				return Err(internal_err(format!(
					"gas required exceeds allowance {}",
					cap
				)))
			}
			// If the transaction reverts, there are two possible cases,
			// it can revert because the called contract feels that it does not have enough
			// gas left to continue, or it can revert for another reason unrelated to gas.
			ExitReason::Revert(revert) => {
				if request.gas.is_some() || request.gas_price.is_some() {
					// If the user has provided a gas limit or a gas price, then we have executed
					// with less block gas limit, so we must reexecute with block gas limit to
					// know if the revert is due to a lack of gas or not.
					let ExecutableResult {
						data,
						exit_reason,
						used_gas: _,
					} = executable(
						request.clone(),
						max_gas_limit,
						api_version,
						client.runtime_api(),
						estimate_mode,
					)?;
					match exit_reason {
						ExitReason::Succeed(_) => {
							return Err(internal_err(format!(
								"gas required exceeds allowance {cap}",
							)))
						}
						// The execution has been done with block gas limit, so it is not a lack of gas from the user.
						other => error_on_execution_failure(&other, &data)?,
					}
				} else {
					// The execution has already been done with block gas limit, so it is not a lack of gas from the user.
					error_on_execution_failure(&ExitReason::Revert(revert), &data)?
				}
			}
			other => error_on_execution_failure(&other, &data)?,
		};

		#[cfg(not(feature = "rpc-binary-search-estimate"))]
		{
			Ok(used_gas)
		}
		#[cfg(feature = "rpc-binary-search-estimate")]
		{
			// On binary search, evm estimate mode is disabled
			let estimate_mode = false;
			// Define the lower bound of the binary search
			let mut lowest = MIN_GAS_PER_TX;

			// Start close to the used gas for faster binary search
			let mut mid = std::cmp::min(used_gas * 3, (highest + lowest) / 2);

			// Execute the binary search and hone in on an executable gas limit.
			let mut previous_highest = highest;
			while (highest - lowest) > U256::one() {
				let ExecutableResult {
					data,
					exit_reason,
					used_gas: _,
				} = executable(
					request.clone(),
					mid,
					api_version,
					client.runtime_api(),
					estimate_mode,
				)?;
				match exit_reason {
					ExitReason::Succeed(_) => {
						highest = mid;
						// If the variation in the estimate is less than 10%,
						// then the estimate is considered sufficiently accurate.
						if (previous_highest - highest) * 10 / previous_highest < U256::one() {
							return Ok(highest);
						}
						previous_highest = highest;
					}
					ExitReason::Revert(_)
					| ExitReason::Error(ExitError::OutOfGas)
					| ExitReason::Error(ExitError::InvalidCode(_)) => {
						lowest = mid;
					}
					other => error_on_execution_failure(&other, &data)?,
				}
				mid = (highest + lowest) / 2;
			}

			Ok(highest)
		}
	}

	/// Given an address mapped `CallStateOverride`, creates `OverlayedChanges` to be used for
	/// `CallApiAt` eth_call.
	fn create_overrides_overlay(
		&self,
		block_hash: B::Hash,
		api_version: u32,
		state_overrides: Option<BTreeMap<H160, CallStateOverride>>,
	) -> RpcResult<OverlayedChanges<HashingFor<B>>> {
		let mut overlayed_changes = OverlayedChanges::default();
		if let Some(state_overrides) = state_overrides {
			for (address, state_override) in state_overrides {
				if EC::RuntimeStorageOverride::is_enabled() {
					EC::RuntimeStorageOverride::set_overlayed_changes(
						self.client.as_ref(),
						&mut overlayed_changes,
						block_hash,
						api_version,
						address,
						state_override.balance,
						state_override.nonce,
					);
				} else if state_override.balance.is_some() || state_override.nonce.is_some() {
					return Err(internal_err(
						"state override unsupported for balance and nonce",
					));
				}

				if let Some(code) = &state_override.code {
					let mut key = [twox_128(PALLET_EVM), twox_128(EVM_ACCOUNT_CODES)]
						.concat()
						.to_vec();
					key.extend(blake2_128(address.as_bytes()));
					key.extend(address.as_bytes());
					let encoded_code = code.clone().into_vec().encode();
					overlayed_changes.set_storage(key.clone(), Some(encoded_code));
				}

				let mut account_storage_key = [
					twox_128(PALLET_EVM),
					twox_128(fp_storage::EVM_ACCOUNT_STORAGES),
				]
				.concat()
				.to_vec();
				account_storage_key.extend(blake2_128(address.as_bytes()));
				account_storage_key.extend(address.as_bytes());

				// Use `state` first. If `stateDiff` is also present, it resolves consistently
				if let Some(state) = &state_override.state {
					// clear all storage
					if let Ok(all_keys) = self.client.storage_keys(
						block_hash,
						Some(&sp_storage::StorageKey(account_storage_key.clone())),
						None,
					) {
						for key in all_keys {
							overlayed_changes.set_storage(key.0, None);
						}
					}
					// set provided storage
					for (k, v) in state {
						let mut slot_key = account_storage_key.clone();
						slot_key.extend(blake2_128(k.as_bytes()));
						slot_key.extend(k.as_bytes());

						overlayed_changes.set_storage(slot_key, Some(v.as_bytes().to_owned()));
					}
				}

				if let Some(state_diff) = &state_override.state_diff {
					for (k, v) in state_diff {
						let mut slot_key = account_storage_key.clone();
						slot_key.extend(blake2_128(k.as_bytes()));
						slot_key.extend(k.as_bytes());

						overlayed_changes.set_storage(slot_key, Some(v.as_bytes().to_owned()));
					}
				}
			}
		}

		Ok(overlayed_changes)
	}
}

pub fn error_on_execution_failure(reason: &ExitReason, data: &[u8]) -> RpcResult<()> {
	match reason {
		ExitReason::Succeed(_) => Ok(()),
		ExitReason::Error(err) => {
			if *err == ExitError::OutOfGas {
				// `ServerError(0)` will be useful in estimate gas
				return Err(internal_err("out of gas"));
			}
			Err(crate::internal_err_with_data(
				format!("evm error: {err:?}"),
				&[],
			))
		}
		ExitReason::Revert(_) => {
			const LEN_START: usize = 36;
			const MESSAGE_START: usize = 68;

			let mut message = "VM Exception while processing transaction: revert".to_string();
			// A minimum size of error function selector (4) + offset (32) + string length (32)
			// should contain a utf-8 encoded revert reason.
			if data.len() > MESSAGE_START {
				let message_len =
					U256::from(&data[LEN_START..MESSAGE_START]).saturated_into::<usize>();
				let message_end = MESSAGE_START.saturating_add(message_len);

				if data.len() >= message_end {
					let body: &[u8] = &data[MESSAGE_START..message_end];
					if let Ok(reason) = std::str::from_utf8(body) {
						message = format!("{message} {reason}");
					}
				}
			}
			Err(crate::internal_err_with_data(message, data))
		}
		ExitReason::Fatal(err) => Err(crate::internal_err_with_data(
			format!("evm fatal: {err:?}"),
			&[],
		)),
	}
}

struct FeeDetails {
	gas_price: Option<U256>,
	max_fee_per_gas: Option<U256>,
	max_priority_fee_per_gas: Option<U256>,
	fee_cap: U256,
}

fn fee_details(
	request_gas_price: Option<U256>,
	request_max_fee_per_gas: Option<U256>,
	request_priority_fee_per_gas: Option<U256>,
) -> RpcResult<FeeDetails> {
	match (
		request_gas_price,
		request_max_fee_per_gas,
		request_priority_fee_per_gas,
	) {
		(Some(_), Some(_), Some(_)) => Err(internal_err(
			"both gasPrice and (maxFeePerGas or maxPriorityFeePerGas) specified",
		)),
		// Legacy or EIP-2930 transaction.
		(gas_price, None, None) if gas_price.is_some() => Ok(FeeDetails {
			gas_price,
			max_fee_per_gas: None,
			max_priority_fee_per_gas: None,
			fee_cap: gas_price.unwrap_or_default(),
		}),
		// EIP-1559 transaction
		(None, Some(max_fee), Some(max_priority)) => {
			if max_priority > max_fee {
				return Err(internal_err(
					"Invalid input: `max_priority_fee_per_gas` greater than `max_fee_per_gas`",
				));
			}
			Ok(FeeDetails {
				gas_price: None,
				max_fee_per_gas: Some(max_fee),
				max_priority_fee_per_gas: Some(max_priority),
				fee_cap: max_fee,
			})
		}
		// Default to EIP-1559 transaction
		_ => Ok(FeeDetails {
			gas_price: None,
			max_fee_per_gas: Some(U256::zero()),
			max_priority_fee_per_gas: Some(U256::zero()),
			fee_cap: U256::zero(),
		}),
	}
}
