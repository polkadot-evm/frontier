// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2022 Parity Technologies (UK) Ltd.
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

use std::sync::Arc;

use ethereum_types::{H256, U256};
use evm::{ExitError, ExitReason};
use jsonrpsee::core::RpcResult as Result;
// Substrate
use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
use sc_network_common::ExHashT;
use sc_transaction_pool::ChainApi;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::{BlockStatus, HeaderBackend};
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT},
	SaturatedConversion,
};
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	eth::{pending_runtime_api, Eth},
	frontier_backend_client, internal_err,
};

/// Default JSONRPC error code return by geth
pub const JSON_RPC_ERROR_DEFAULT: i32 = -32000;

/// Allow to adapt a request for `estimate_gas`.
/// Can be used to estimate gas of some contracts using a different function
/// in the case the normal gas estimation doesn't work.
///
/// Exemple: a precompile that tries to do a subcall but succeeds regardless of the
/// success of the subcall. The gas estimation will thus optimize the gas limit down
/// to the minimum, while we want to estimate a gas limit that will allow the subcall to
/// have enough gas to succeed.
pub trait EstimateGasAdapter {
	fn adapt_request(request: CallRequest) -> CallRequest;
}

impl EstimateGasAdapter for () {
	fn adapt_request(request: CallRequest) -> CallRequest {
		request
	}
}

impl<B, C, P, CT, BE, H: ExHashT, A: ChainApi, EGA> Eth<B, C, P, CT, BE, H, A, EGA>
where
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	C: ProvideRuntimeApi<B> + StorageProvider<B, BE>,
	C: HeaderBackend<B> + Send + Sync + 'static,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	A: ChainApi<Block = B> + 'static,
	EGA: EstimateGasAdapter,
{
	pub fn call(&self, request: CallRequest, number: Option<BlockNumber>) -> Result<Bytes> {
		let CallRequest {
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

		let (id, api) = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number,
		)? {
			Some(id) => (id, self.client.runtime_api()),
			None => {
				// Not mapped in the db, assume pending.
				let id = BlockId::Hash(self.client.info().best_hash);
				let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
				(id, api)
			}
		};

		if let Ok(BlockStatus::Unknown) = self.client.status(id) {
			return Err(crate::err(JSON_RPC_ERROR_DEFAULT, "header not found", None));
		}

		let api_version =
			if let Ok(Some(api_version)) = api.api_version::<dyn EthereumRuntimeRPCApi<B>>(&id) {
				api_version
			} else {
				return Err(internal_err("failed to retrieve Runtime Api version"));
			};

		let block = if api_version > 1 {
			api.current_block(&id)
				.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
		} else {
			#[allow(deprecated)]
			let legacy_block = api
				.current_block_before_version_2(&id)
				.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
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
			None => match api.gas_limit_multiplier_support(&id) {
				Ok(_) => max_gas_limit,
				_ => block_gas_limit,
			},
		};

		let data = data.map(|d| d.0).unwrap_or_default();
		match to {
			Some(to) => {
				if api_version == 1 {
					// Legacy pre-london
					#[allow(deprecated)]
					let info = api.call_before_version_2(
						&id,
						from.unwrap_or_default(),
						to,
						data,
						value.unwrap_or_default(),
						gas_limit,
						gas_price,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else if api_version >= 2 && api_version < 4 {
					// Post-london
					#[allow(deprecated)]
					let info = api.call_before_version_4(
						&id,
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
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else if api_version == 4 {
					// Post-london + access list support
					let access_list = access_list.unwrap_or_default();
					let info = api
						.call(
							&id,
							from.unwrap_or_default(),
							to,
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
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
						.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else {
					Err(internal_err("failed to retrieve Runtime Api version"))
				}
			}
			None => {
				if api_version == 1 {
					// Legacy pre-london
					#[allow(deprecated)]
					let info = api.create_before_version_2(
						&id,
						from.unwrap_or_default(),
						data,
						value.unwrap_or_default(),
						gas_limit,
						gas_price,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(&id, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else if api_version >= 2 && api_version < 4 {
					// Post-london
					#[allow(deprecated)]
					let info = api.create_before_version_4(
						&id,
						from.unwrap_or_default(),
						data,
						value.unwrap_or_default(),
						gas_limit,
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						false,
					)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(&id, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else if api_version == 4 {
					// Post-london + access list support
					let access_list = access_list.unwrap_or_default();
					let info = api
						.create(
							&id,
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
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
						.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(&id, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else {
					Err(internal_err("failed to retrieve Runtime Api version"))
				}
			}
		}
	}

	pub async fn estimate_gas(&self, request: CallRequest, _: Option<BlockNumber>) -> Result<U256> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);

		// Define the lower bound of estimate
		const MIN_GAS_PER_TX: U256 = U256([21_000, 0, 0, 0]);

		// Get best hash (TODO missing support for estimating gas historically)
		let best_hash = client.info().best_hash;

		// Adapt request for gas estimation.
		let request = EGA::adapt_request(request);

		// For simple transfer to simple account, return MIN_GAS_PER_TX directly
		let is_simple_transfer = match &request.data {
			None => true,
			Some(vec) => vec.0.is_empty(),
		};
		if is_simple_transfer {
			if let Some(to) = request.to {
				let to_code = client
					.runtime_api()
					.account_code_at(&BlockId::Hash(best_hash), to)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
				if to_code.is_empty() {
					return Ok(MIN_GAS_PER_TX);
				}
			}
		}

		let (gas_price, max_fee_per_gas, max_priority_fee_per_gas) = {
			let details = fee_details(
				request.gas_price,
				request.max_fee_per_gas,
				request.max_priority_fee_per_gas,
			)?;
			(
				details.gas_price,
				details.max_fee_per_gas,
				details.max_priority_fee_per_gas,
			)
		};

		let block_gas_limit = {
			let substrate_hash = client.info().best_hash;
			let id = BlockId::Hash(substrate_hash);
			let schema = frontier_backend_client::onchain_storage_schema::<B, C, BE>(&client, id);
			let block = block_data_cache.current_block(schema, substrate_hash).await;

			block
				.ok_or_else(|| internal_err("block unavailable, cannot query gas limit"))?
				.header
				.gas_limit
		};

		let max_gas_limit = block_gas_limit * self.execute_gas_limit_multiplier;

		let api = client.runtime_api();

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
			None => match api.gas_limit_multiplier_support(&BlockId::Hash(best_hash)) {
				Ok(_) => max_gas_limit,
				_ => block_gas_limit,
			},
		};

		// Recap the highest gas allowance with account's balance.
		if let Some(from) = request.from {
			let gas_price = gas_price.unwrap_or_default();
			if gas_price > U256::zero() {
				let balance = api
					.account_basic(&BlockId::Hash(best_hash), from)
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.balance;
				let mut available = balance;
				if let Some(value) = request.value {
					if value > available {
						return Err(internal_err("insufficient funds for transfer"));
					}
					available -= value;
				}
				let allowance = available / gas_price;
				if highest > allowance {
					log::warn!(
							"Gas estimation capped by limited funds original {} balance {} sent {} feecap {} fundable {}",
							highest,
							balance,
							request.value.unwrap_or_default(),
							gas_price,
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
			| -> Result<ExecutableResult> {
				let CallRequest {
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

				let data = data.map(|d| d.0).unwrap_or_default();

				let (exit_reason, data, used_gas) = match to {
					Some(to) => {
						let info = if api_version == 1 {
							// Legacy pre-london
							#[allow(deprecated)]
							api.call_before_version_2(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								to,
								data,
								value.unwrap_or_default(),
								gas_limit,
								gas_price,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else if api_version < 4 {
							// Post-london
							#[allow(deprecated)]
							api.call_before_version_4(
								&BlockId::Hash(best_hash),
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
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							api.call(
								&BlockId::Hash(best_hash),
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
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						};

						(info.exit_reason, info.value, info.used_gas)
					}
					None => {
						let info = if api_version == 1 {
							// Legacy pre-london
							#[allow(deprecated)]
							api.create_before_version_2(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								gas_price,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else if api_version < 4 {
							// Post-london
							#[allow(deprecated)]
							api.create_before_version_4(
								&BlockId::Hash(best_hash),
								from.unwrap_or_default(),
								data,
								value.unwrap_or_default(),
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								estimate_mode,
							)
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						} else {
							// Post-london + access list support
							let access_list = access_list.unwrap_or_default();
							api.create(
								&BlockId::Hash(best_hash),
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
							.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
							.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?
						};

						(info.exit_reason, Vec::new(), info.used_gas)
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
				.api_version::<dyn EthereumRuntimeRPCApi<B>>(&BlockId::Hash(best_hash))
		{
			api_version
		} else {
			return Err(internal_err("failed to retrieve Runtime Api version"));
		};

		// Verify that the transaction succeed with highest capacity
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
								"gas required exceeds allowance {}",
								cap
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
}

pub fn error_on_execution_failure(reason: &ExitReason, data: &[u8]) -> Result<()> {
	match reason {
		ExitReason::Succeed(_) => Ok(()),
		ExitReason::Error(e) => {
			if *e == ExitError::OutOfGas {
				// `ServerError(0)` will be useful in estimate gas
				return Err(internal_err("out of gas"));
			}
			Err(crate::internal_err_with_data(
				format!("evm error: {:?}", e),
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
						message = format!("{} {}", message, reason);
					}
				}
			}
			Err(crate::internal_err_with_data(message, data))
		}
		ExitReason::Fatal(e) => Err(crate::internal_err_with_data(
			format!("evm fatal: {:?}", e),
			&[],
		)),
	}
}

struct FeeDetails {
	gas_price: Option<U256>,
	max_fee_per_gas: Option<U256>,
	max_priority_fee_per_gas: Option<U256>,
}

fn fee_details(
	request_gas_price: Option<U256>,
	request_max_fee: Option<U256>,
	request_priority: Option<U256>,
) -> Result<FeeDetails> {
	match (request_gas_price, request_max_fee, request_priority) {
		(gas_price, None, None) => {
			// Legacy request, all default to gas price.
			// A zero-set gas price is None.
			let gas_price = if gas_price.unwrap_or_default().is_zero() {
				None
			} else {
				gas_price
			};
			Ok(FeeDetails {
				gas_price,
				max_fee_per_gas: gas_price,
				max_priority_fee_per_gas: gas_price,
			})
		}
		(_, max_fee, max_priority) => {
			// eip-1559
			// A zero-set max fee is None.
			let max_fee = if max_fee.unwrap_or_default().is_zero() {
				None
			} else {
				max_fee
			};
			// Ensure `max_priority_fee_per_gas` is less or equal to `max_fee_per_gas`.
			if let Some(max_priority) = max_priority {
				let max_fee = max_fee.unwrap_or_default();
				if max_priority > max_fee {
					return Err(internal_err(
						"Invalid input: `max_priority_fee_per_gas` greater than `max_fee_per_gas`",
					));
				}
			}
			Ok(FeeDetails {
				gas_price: max_fee,
				max_fee_per_gas: max_fee,
				max_priority_fee_per_gas: max_priority,
			})
		}
	}
}
