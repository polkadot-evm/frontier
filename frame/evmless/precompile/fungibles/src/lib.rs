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
	ExitRevert, ExitSucceed, Precompile, PrecompileFailure, PrecompileHandle, PrecompileResult,
};
use frame_support::{
	sp_runtime::Saturating,
	traits::tokens::fungibles::{
		approvals::{Inspect as ApprovalInspect, Mutate},
		Inspect, InspectMetadata, Transfer,
	},
};
use precompile_utils::handle::PrecompileHandleExt;
use precompile_utils::prelude::*;
use sp_core::{H160, U256};

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
	R: pallet_evmless::Config,
	AssetIdOf<R>: From<u32>,
	BalanceOf<R>: EvmData + Into<U256>,
	<R as frame_system::Config>::AccountId: From<H160>,
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
			ERC20Methods::Allowance => Self::allowance(handle),
			ERC20Methods::Transfer => Self::transfer(handle),
			ERC20Methods::Approve => Self::approve(handle),
			ERC20Methods::TransferFrom => Self::transfer_from(handle),
			ERC20Methods::Name => Self::name(handle),
			ERC20Methods::Symbol => Self::symbol(handle),
			ERC20Methods::Decimals => Self::decimals(handle),
		}
	}
}

pub type AssetIdOf<R> = <<R as pallet_evmless::Config>::Fungibles as Inspect<
	<R as frame_system::Config>::AccountId,
>>::AssetId;

pub type BalanceOf<R> = <<R as pallet_evmless::Config>::Fungibles as Inspect<
	<R as frame_system::Config>::AccountId,
>>::Balance;

impl<R> Fungibles<R>
where
	R: pallet_evmless::Config,
	AssetIdOf<R>: From<u32>,
	BalanceOf<R>: EvmData + Into<U256>,
	<R as frame_system::Config>::AccountId: From<H160>,
{
	fn total_supply(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let t = R::Fungibles::total_issuance(0u32.into());

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(t).build(),
		})
	}

	fn name(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let name: UnboundedBytes = R::Fungibles::name(&0u32.into()).as_slice().into();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(name).build(),
		})
	}

	fn symbol(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let symbol: UnboundedBytes = R::Fungibles::symbol(&0u32.into()).as_slice().into();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(symbol).build(),
		})
	}

	fn decimals(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let d = R::Fungibles::decimals(&0u32.into());

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
		let balance = R::Fungibles::balance(0u32.into(), &who);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(balance).build(),
		})
	}

	fn transfer(handle: &mut impl PrecompileHandleExt) -> EvmResult<PrecompileOutput> {
		handle.record_log_costs_manual(3, 32)?;

		let mut input = handle.read_after_selector()?;
		input.expect_arguments(2)?;

		let origin: H160 = handle.context().caller;
		let to: H160 = input.read::<Address>()?.into();

		let amount = input.read::<BalanceOf<R>>()?;

		// keep_alive is set to false, so this might kill origin
		R::Fungibles::transfer(
			0u32.into(),
			&origin.into(),
			&to.into(),
			amount.try_into().ok().unwrap(),
			false,
		)
		.map_err(|e| PrecompileFailure::Revert {
			exit_status: ExitRevert::Reverted,
			output: Into::<&str>::into(e).as_bytes().to_vec(),
		})?;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(true).build(),
		})
	}

	fn approve(handle: &mut impl PrecompileHandleExt) -> EvmResult<PrecompileOutput> {
		handle.record_log_costs_manual(3, 32)?;

		let mut input = handle.read_after_selector()?;
		input.expect_arguments(2)?;

		let origin = R::AddressMapping::into_account_id(handle.context().caller);
		let spender: H160 = input.read::<Address>()?.into();

		let amount = input.read::<BalanceOf<R>>()?;

		// if previous approval exists, we need to clean it
		if R::Fungibles::allowance(0u32.into(), &origin, &spender.into()) != 0u32.into() {
			R::Fungibles::approve(
				0u32.into(),
				&origin.clone().into(),
				&spender.into(),
				0u32.into(),
			)
			.map_err(|e| PrecompileFailure::Revert {
				exit_status: ExitRevert::Reverted,
				output: Into::<&str>::into(e).as_bytes().to_vec(),
			})?;
		}

		R::Fungibles::approve(0u32.into(), &origin.into(), &spender.into(), amount).map_err(
			|e| PrecompileFailure::Revert {
				exit_status: ExitRevert::Reverted,
				output: Into::<&str>::into(e).as_bytes().to_vec(),
			},
		)?;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(true).build(),
		})
	}

	fn allowance(handle: &mut impl PrecompileHandleExt) -> EvmResult<PrecompileOutput> {
		handle.record_cost(RuntimeHelper::<R>::db_read_gas_cost())?;

		let mut input = handle.read_after_selector()?;
		input.expect_arguments(2)?;

		let owner: H160 = input.read::<Address>()?.into();
		let spender: H160 = input.read::<Address>()?.into();

		let amount: U256 = {
			let owner: R::AccountId = R::AddressMapping::into_account_id(owner);
			let spender: R::AccountId = R::AddressMapping::into_account_id(spender);

			R::Fungibles::allowance(0u32.into(), &owner, &spender).into()
		};

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(amount).build(),
		})
	}

	fn transfer_from(handle: &mut impl PrecompileHandleExt) -> EvmResult<PrecompileOutput> {
		handle.record_log_costs_manual(3, 32)?;

		let mut input = handle.read_after_selector()?;
		input.expect_arguments(3)?;

		let origin = R::AddressMapping::into_account_id(handle.context().caller);

		let from: H160 = input.read::<Address>()?.into();
		let to: H160 = input.read::<Address>()?.into();
		let amount = input.read::<BalanceOf<R>>()?;

		// spender is not caller
		if origin != from.into() {
			let allowance_before = R::Fungibles::allowance(0u32.into(), &from.into(), &origin);

			R::Fungibles::transfer_from(
				0u32.into(),
				&from.into(),
				&origin.clone().into(),
				&to.into(),
				amount,
			)
			.map_err(|e| PrecompileFailure::Revert {
				exit_status: ExitRevert::Reverted,
				output: Into::<&str>::into(e).as_bytes().to_vec(),
			})?;

			R::Fungibles::approve(
				0u32.into(),
				&from.into(),
				&origin.into(),
				allowance_before.saturating_sub(amount),
			)
			.map_err(|e| PrecompileFailure::Revert {
				exit_status: ExitRevert::Reverted,
				output: Into::<&str>::into(e).as_bytes().to_vec(),
			})?;
		} else {
			R::Fungibles::transfer(
				0u32.into(),
				&origin.into(),
				&to.into(),
				amount.try_into().ok().unwrap(),
				false,
			)
			.map_err(|e| PrecompileFailure::Revert {
				exit_status: ExitRevert::Reverted,
				output: Into::<&str>::into(e).as_bytes().to_vec(),
			})?;
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: EvmDataWriter::new().write(true).build(),
		})
	}
}
