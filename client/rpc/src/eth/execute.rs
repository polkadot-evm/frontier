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

use ethereum_types::U256;
use evm::{ExitError, ExitReason};
use jsonrpsee::core::RpcResult as Result;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_network_common::ExHashT;
use sc_transaction_pool::ChainApi;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{traits::Block as BlockT, SaturatedConversion};
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
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
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

		let (substrate_hash, api) = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			number,
		)? {
			Some(id) => {
				let hash = self
					.client
					.expect_block_hash_from_id(&id)
					.map_err(|_| crate::err(JSON_RPC_ERROR_DEFAULT, "header not found", None))?;
				(hash, self.client.runtime_api())
			}
			None => {
				// Not mapped in the db, assume pending.
				let hash = self.client.info().best_hash;
				let api = pending_runtime_api(self.client.as_ref(), self.graph.as_ref())?;
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
				.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
		} else {
			#[allow(deprecated)]
			let legacy_block = api
				.current_block_before_version_2(substrate_hash)
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
			None => match api.gas_limit_multiplier_support(substrate_hash) {
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
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

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
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &info.value)?;
					Ok(Bytes(info.value))
				} else if api_version == 4 {
					// Post-london + access list support
					let access_list = access_list.unwrap_or_default();
					let info = api
						.call(
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
						substrate_hash,
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
						.account_code_at(substrate_hash, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
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
					.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
					.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(substrate_hash, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else if api_version == 4 {
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
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?
						.map_err(|err| internal_err(format!("execution fatal: {:?}", err)))?;

					error_on_execution_failure(&info.exit_reason, &[])?;

					let code = api
						.account_code_at(substrate_hash, info.value)
						.map_err(|err| internal_err(format!("runtime error: {:?}", err)))?;
					Ok(Bytes(code))
				} else {
					Err(internal_err("failed to retrieve Runtime Api version"))
				}
			}
		}
	}

	pub async fn estimate_gas(&self, request: CallRequest, _: Option<BlockNumber>) -> Result<U256> {
		Ok(U256::one())
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
