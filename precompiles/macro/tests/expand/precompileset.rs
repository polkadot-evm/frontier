// This file is part of Frontier.

// Copyright (c) Moonsong Labs.
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

use precompile_utils::{prelude::*, testing::PrecompileTesterExt, EvmResult};
use sp_core::H160;

// Based on Erc20AssetsPrecompileSet with stripped code.
// Simplified to use concrete types for proper macro expansion testing.

struct PrecompileSet;

type Discriminant = u32;
type StringLimit = ConstU32<42>;

#[precompile_utils_macro::precompile]
#[precompile::precompile_set]
impl PrecompileSet {
	/// PrecompileSet discriminant. Allows to know if the address maps to an asset id,
	/// and if this is the case which one.
	#[precompile::discriminant]
	fn discriminant(address: H160, gas: u64) -> DiscriminantResult<Discriminant> {
		DiscriminantResult::Some(1u32, gas)
	}

	#[precompile::public("totalSupply()")]
	fn total_supply(asset_id: Discriminant, handle: &mut impl PrecompileHandle) -> EvmResult<U256> {
		todo!("total_supply")
	}

	#[precompile::public("balanceOf(address)")]
	fn balance_of(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		who: Address,
	) -> EvmResult<U256> {
		todo!("balance_of")
	}

	#[precompile::public("allowance(address,address)")]
	fn allowance(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		owner: Address,
		spender: Address,
	) -> EvmResult<U256> {
		todo!("allowance")
	}

	#[precompile::public("approve(address,uint256)")]
	fn approve(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		spender: Address,
		value: U256,
	) -> EvmResult<bool> {
		todo!("approve")
	}

	#[precompile::public("transfer(address,uint256)")]
	fn transfer(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		to: Address,
		value: U256,
	) -> EvmResult<bool> {
		todo!("transfer")
	}

	#[precompile::public("transferFrom(address,address,uint256)")]
	fn transfer_from(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		from: Address,
		to: Address,
		value: U256,
	) -> EvmResult<bool> {
		todo!("transfer_from")
	}

	#[precompile::public("name()")]
	fn name(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<UnboundedBytes> {
		todo!("name")
	}

	#[precompile::public("symbol()")]
	fn symbol(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<UnboundedBytes> {
		todo!("symbol")
	}

	#[precompile::public("decimals()")]
	fn decimals(asset_id: Discriminant, handle: &mut impl PrecompileHandle) -> EvmResult<u8> {
		todo!("decimals")
	}

	#[precompile::public("mint(address,uint256)")]
	fn mint(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		to: Address,
		value: U256,
	) -> EvmResult<bool> {
		todo!("mint")
	}

	#[precompile::public("burn(address,uint256)")]
	fn burn(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		from: Address,
		value: U256,
	) -> EvmResult<bool> {
		todo!("burn")
	}

	#[precompile::public("freeze(address)")]
	fn freeze(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		account: Address,
	) -> EvmResult<bool> {
		todo!("freeze")
	}

	#[precompile::public("thaw(address)")]
	fn thaw(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		account: Address,
	) -> EvmResult<bool> {
		todo!("thaw")
	}

	#[precompile::public("freezeAsset()")]
	#[precompile::public("freeze_asset()")]
	fn freeze_asset(asset_id: Discriminant, handle: &mut impl PrecompileHandle) -> EvmResult<bool> {
		todo!("freeze_asset")
	}

	#[precompile::public("thawAsset()")]
	#[precompile::public("thaw_asset()")]
	fn thaw_asset(asset_id: Discriminant, handle: &mut impl PrecompileHandle) -> EvmResult<bool> {
		todo!("thaw_asset")
	}

	#[precompile::public("transferOwnership(address)")]
	#[precompile::public("transfer_ownership(address)")]
	fn transfer_ownership(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		owner: Address,
	) -> EvmResult<bool> {
		todo!("transfer_ownership")
	}

	#[precompile::public("setTeam(address,address,address)")]
	#[precompile::public("set_team(address,address,address)")]
	fn set_team(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		issuer: Address,
		admin: Address,
		freezer: Address,
	) -> EvmResult<bool> {
		todo!("set_team")
	}

	#[precompile::public("setMetadata(string,string,uint8)")]
	#[precompile::public("set_metadata(string,string,uint8)")]
	fn set_metadata(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		name: BoundedString<StringLimit>,
		symbol: BoundedString<StringLimit>,
		decimals: u8,
	) -> EvmResult<bool> {
		todo!("set_metadata")
	}

	#[precompile::public("clearMetadata()")]
	#[precompile::public("clear_metadata()")]
	fn clear_metadata(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<bool> {
		todo!("clear_metadata")
	}

	#[precompile::public("permit(address,address,uint256,uint256,uint8,bytes32,bytes32)")]
	fn eip2612_permit(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		owner: Address,
		spender: Address,
		value: U256,
		deadline: U256,
		v: u8,
		r: H256,
		s: H256,
	) -> EvmResult {
		todo!("eip2612_permit")
	}

	#[precompile::public("nonces(address)")]
	#[precompile::view]
	fn eip2612_nonces(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
		owner: Address,
	) -> EvmResult<U256> {
		todo!("eip2612_nonces")
	}

	#[precompile::public("DOMAIN_SEPARATOR()")]
	#[precompile::view]
	fn eip2612_domain_separator(
		asset_id: Discriminant,
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<H256> {
		todo!("eip2612_domain_separator")
	}
}
