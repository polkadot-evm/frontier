// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
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

use crate::{
	runner::Runner as RunnerT, AddressMapping, BalanceOf, Config, Error, Pallet, RunnerError,
};
use evm::{ExitReason, ExitSucceed};
use fp_account::AccountId20;
use fp_evm::{CallInfo, CreateInfo, FeeCalculator, UsedGas, WeightInfo};
use frame_support::{traits::tokens::fungible::Inspect, weights::Weight};
use sp_core::{Get, H160, H256, U256};
use sp_std::marker::PhantomData;

#[derive(Default)]
pub struct Runner<T: Config> {
	_marker: PhantomData<T>,
}

impl<T: Config<AccountId = AccountId20>> RunnerT<T> for Runner<T>
where
	BalanceOf<T>: TryFrom<U256> + Into<U256>,
	<<T as Config>::Currency as Inspect<T::AccountId>>::Balance: TryFrom<U256>,
	T: pallet_contracts::Config<Currency = <T as Config>::Currency>,
{
	type Error = Error<T>;

	fn validate(
		source: H160,
		target: Option<H160>,
		input: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		evm_config: &evm::Config,
	) -> Result<(), RunnerError<Self::Error>> {
		let (base_fee, mut weight) = T::FeeCalculator::min_gas_price();
		let (source_account, inner_weight) = Pallet::<T>::account_basic(&source);
		weight = weight.saturating_add(inner_weight);

		let _ = fp_evm::CheckEvmTransaction::<Self::Error>::new(
			fp_evm::CheckEvmTransactionConfig {
				evm_config,
				block_gas_limit: T::BlockGasLimit::get(),
				base_fee,
				chain_id: T::ChainId::get(),
				is_transactional,
			},
			fp_evm::CheckEvmTransactionInput {
				chain_id: Some(T::ChainId::get()),
				to: target,
				input,
				nonce: nonce.unwrap_or(source_account.nonce),
				gas_limit: gas_limit.into(),
				gas_price: None,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				value,
				access_list,
			},
			weight_limit,
			proof_size_base_cost,
		)
		.validate_in_block_for(&source_account)
		.and_then(|v| v.with_base_fee())
		.and_then(|v| v.with_balance_for(&source_account))
		.map_err(|error| RunnerError { error, weight })?;
		Ok(())
	}

	fn call(
		source: H160,
		target: H160,
		input: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &evm::Config,
	) -> Result<CallInfo, RunnerError<Self::Error>> {
		if validate {
			Self::validate(
				source,
				Some(target),
				input.clone(),
				value,
				gas_limit,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				nonce,
				access_list.clone(),
				is_transactional,
				weight_limit,
				proof_size_base_cost,
				config,
			)?;
		}
		let origin = T::AddressMapping::into_account_id(source);
		let dest = T::AddressMapping::into_account_id(target);
		let (_base_fee, weight) = T::FeeCalculator::min_gas_price();
		let value = value.try_into().map_err(|_| RunnerError {
			error: Error::<T>::BalanceLow,
			weight,
		})?;
		let ret = pallet_contracts::Pallet::<T>::bare_call(
			origin,
			dest,
			value,
			Weight::from_parts(gas_limit, u64::from(T::MaxCodeLen::get()) * 2),
			None,
			input,
			pallet_contracts::DebugInfo::Skip,
			pallet_contracts::CollectEvents::Skip,
			pallet_contracts::Determinism::Enforced,
		);
		let retd = ret.result.map_err(|_| RunnerError {
			error: Error::<T>::Undefined, // TODO: pallet contracts specific error.
			weight: ret.gas_consumed,
		})?;
		let info = CallInfo {
			exit_reason: ExitReason::Succeed(ExitSucceed::Stopped),
			value: retd.data,
			used_gas: UsedGas {
				standard: ret.gas_consumed.ref_time().into(),
				effective: ret.gas_consumed.ref_time().into(),
			},
			logs: Vec::new(), // TODO: we need to collect logs.
			weight_info: Some(WeightInfo {
				ref_time_limit: Some(ret.gas_required.ref_time()),
				proof_size_limit: Some(ret.gas_required.proof_size()),
				ref_time_usage: Some(ret.gas_consumed.ref_time()),
				proof_size_usage: Some(ret.gas_consumed.proof_size()),
			}),
		};
		Ok(info)
	}

	fn create(
		source: H160,
		init: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &evm::Config,
	) -> Result<CreateInfo, RunnerError<Self::Error>> {
		if validate {
			Self::validate(
				source,
				None,
				init.clone(),
				value,
				gas_limit,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				nonce,
				access_list.clone(),
				is_transactional,
				weight_limit,
				proof_size_base_cost,
				config,
			)?;
		}
		let (_base_fee, weight) = T::FeeCalculator::min_gas_price();
		let (code, init_data, salt): (Vec<u8>, Vec<u8>, Vec<u8>) =
			scale_codec::Decode::decode(&mut &init[..]).map_err(|_| RunnerError {
				error: Error::<T>::Undefined, // TODO: pallet contracts specific error.
				weight,
			})?;
		let origin = T::AddressMapping::into_account_id(source);
		let value = value.try_into().map_err(|_| RunnerError {
			error: Error::<T>::BalanceLow,
			weight,
		})?;
		let ret = pallet_contracts::Pallet::<T>::bare_instantiate(
			origin,
			value,
			Weight::from_parts(gas_limit, u64::from(T::MaxCodeLen::get()) * 2),
			None,
			pallet_contracts::Code::Upload(code),
			init_data,
			salt,
			pallet_contracts::DebugInfo::Skip,
			pallet_contracts::CollectEvents::Skip,
		);
		let retd = ret.result.map_err(|_| RunnerError {
			error: Error::<T>::Undefined, // TODO: pallet contracts specific error.
			weight: ret.gas_consumed,
		})?;
		let info = CreateInfo {
			exit_reason: ExitReason::Succeed(ExitSucceed::Stopped),
			value: retd.account_id.into(),
			used_gas: UsedGas {
				standard: ret.gas_consumed.ref_time().into(),
				effective: ret.gas_consumed.ref_time().into(),
			},
			logs: Vec::new(), // TODO: we need to collect logs.
			weight_info: Some(WeightInfo {
				ref_time_limit: Some(ret.gas_required.ref_time()),
				proof_size_limit: Some(ret.gas_required.proof_size()),
				ref_time_usage: Some(ret.gas_consumed.ref_time()),
				proof_size_usage: Some(ret.gas_consumed.proof_size()),
			}),
		};
		Ok(info)
	}

	fn create2(
		source: H160,
		init: Vec<u8>,
		_salt: H256,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &evm::Config,
	) -> Result<CreateInfo, RunnerError<Self::Error>> {
		if validate {
			Self::validate(
				source,
				None,
				init.clone(),
				value,
				gas_limit,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				nonce,
				access_list.clone(),
				is_transactional,
				weight_limit,
				proof_size_base_cost,
				config,
			)?;
		}
		let (_base_fee, weight) = T::FeeCalculator::min_gas_price();
		return Err(RunnerError {
			error: Error::<T>::Undefined, // TODO: pallet contracts specific error.
			weight,
		});
	}
}
