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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

// #[cfg(test)]
// mod mock;

// #[cfg(test)]
// mod tests;

use core::marker::PhantomData;
use fp_evm::{
	ExitSucceed, Precompile, PrecompileHandle, PrecompileResult,
};
use frame_support::traits::tokens::fungibles::{Inspect, InspectMetadata};
use frame_support::{sp_runtime::traits::StaticLookup, dispatch::{Dispatchable, PostDispatchInfo, GetDispatchInfo}};
use precompile_utils::handle::PrecompileHandleExt;
use precompile_utils::prelude::*;
use sp_core::H160;

use pallet_evmless::AddressMapping;

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
pub enum ERC20Methods {
	TotalSupply = "totalSupply()",
	BalanceOf = "balanceOf(address)",
	Allowance = "allowance(address,address)",
	Transfer = "transfer(address,uint256)",
	Approve = "approve(address,uint256)",
	TransferFrom = "transferFrom(address,address,uint256)",
	Name = "name()",
	Symbol = "symbol()",
	Decimals = "decimals()",
}

pub struct Fungibles<R>(PhantomData<R>);

impl<R> Precompile for Fungibles<R>
where
	R: pallet_evmless::Config + pallet_assets::Config,
	AssetIdParameterOf<R>: From<u32>,
	AssetIdOf<R>: From<u32>,
	BalanceOf<R>: EvmData,
	<R as frame_system::Config>::AccountId: From<H160>,
	//<<R as frame_system::Config>::Lookup as StaticLookup>::Source: From<H160>,
	R::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	R::RuntimeCall: From<pallet_assets::Call<R>>,
	<R::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<R::AccountId>>,
{
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		// todo: check address
		//let address = handle.code_address();

		let selector = match handle.read_selector() {
			Ok(selector) => selector,
			Err(e) => return Err(e.into()),
		};

		if let Err(err) = handle.check_function_modifier(match selector {
			ERC20Methods::Approve | ERC20Methods::Transfer | ERC20Methods::TransferFrom => {
				FunctionModifier::NonPayable
			}
			_ => FunctionModifier::View,
		}) {
			return Err(err.into());
		}

		// todo: change to appropriate method implementations
		match selector {
			ERC20Methods::TotalSupply => Self::total_supply(handle),
			ERC20Methods::BalanceOf => Self::balance_of(handle),
			ERC20Methods::Allowance => Self::total_supply(handle),
			ERC20Methods::Transfer => Self::transfer(handle),
			ERC20Methods::Approve => Self::total_supply(handle),
			ERC20Methods::TransferFrom => Self::total_supply(handle),
			ERC20Methods::Name => Self::name(handle),
			ERC20Methods::Symbol => Self::symbol(handle),
			ERC20Methods::Decimals => Self::decimals(handle),
		}
	}
}

pub type AssetIdOf<R> = <R as pallet_assets::Config>::AssetId;
pub type AssetIdParameterOf<R> = <R as pallet_assets::Config>::AssetIdParameter;
pub type BalanceOf<R> = <R as pallet_assets::Config>::Balance;

impl<R> Fungibles<R>
where
	R: pallet_evmless::Config + pallet_assets::Config,
	AssetIdParameterOf<R>: From<u32>,
	AssetIdOf<R>: From<u32>,
	BalanceOf<R>: EvmData,
	<R as frame_system::Config>::AccountId: From<H160>,
	R::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	R::RuntimeCall: From<pallet_assets::Call<R>>,
	<R::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<R::AccountId>>,
{
	fn total_supply(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let t = pallet_assets::Pallet::<R>::total_issuance(0u32.into());

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(t).build(),
		})
	}

	fn name(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let name: UnboundedBytes = pallet_assets::Pallet::<R>::name(&0u32.into()).as_slice().into();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(name).build(),
		})
	}

	fn symbol(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let symbol: UnboundedBytes = pallet_assets::Pallet::<R>::symbol(&0u32.into()).as_slice().into();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(symbol).build(),
		})
	}

	fn decimals(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let d = pallet_assets::Pallet::<R>::decimals(&0u32.into());

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(d).build(),
		})
	}

	fn balance_of(handle: &mut impl PrecompileHandleExt) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let mut input = handle.read_after_selector()?;
		input.expect_arguments(1)?;

		let owner: H160 = input.read::<Address>()?.into();
		let who: R::AccountId = owner.into();
		let balance = pallet_assets::Pallet::<R>::balance(0u32.into(), &who);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(balance).build(),
		})
	}

	fn transfer(handle: &mut impl PrecompileHandleExt) -> EvmResult<PrecompileOutput> {
		handle.record_log_costs_manual(3, 32)?;

		let mut input = handle.read_after_selector()?;
		input.expect_arguments(2)?;

		let origin = R::AddressMapping::into_account_id(handle.context().caller);
		let to: H160 = input.read::<Address>()?.into();

		let amount = input.read::<BalanceOf<R>>()?;

		RuntimeHelper::<R>::try_dispatch(
			handle,
			Some(origin).into(),
			pallet_assets::Call::<R>::transfer {
				id: 0u32.into(),
				target: R::Lookup::unlookup(to.into()),
				amount: amount.try_into().ok().unwrap(),
			},
		)?;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(true).build(),
		})
	}
}
