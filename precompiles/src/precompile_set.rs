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

//! Provide utils to assemble precompiles and precompilesets into a
//! final precompile set with security checks. All security checks are enabled by
//! default and must be disabled explicely through type annotations.

use crate::{
	evm::handle::PrecompileHandleExt,
	solidity::{codec::String, revert::revert},
	EvmResult,
};
use alloc::{collections::btree_map::BTreeMap, vec, vec::Vec};
use core::{cell::RefCell, marker::PhantomData, ops::RangeInclusive};
use fp_evm::{
	ExitError, IsPrecompileResult, Precompile, PrecompileFailure, PrecompileHandle,
	PrecompileResult, PrecompileSet, ACCOUNT_CODES_METADATA_PROOF_SIZE,
};
use frame_support::pallet_prelude::Get;
use impl_trait_for_tuples::impl_for_tuples;
use pallet_evm::AddressMapping;
use sp_core::{H160, H256};

/// Trait representing checks that can be made on a precompile call.
/// Types implementing this trait are made to be chained in a tuple.
///
/// For that reason every method returns an Option, None meaning that
/// the implementor have no constraint and the decision is left to
/// latter elements in the chain. If None is returned by all elements of
/// the chain then sensible defaults are used.
///
/// Both `PrecompileAt` and `PrecompileSetStartingWith` have a type parameter that must
/// implement this trait to configure the checks of the precompile(set) it represents.
pub trait PrecompileChecks {
	#[inline(always)]
	/// Is there a limit to the amount of recursions this precompile
	/// can make using subcalls? 0 means this specific precompile will not
	/// be callable as a subcall of itself, 1 will allow one level of recursion,
	/// etc...
	///
	/// If all checks return None, defaults to `Some(0)` (no recursion allowed).
	fn recursion_limit() -> Option<Option<u16>> {
		None
	}

	#[inline(always)]
	/// Does this precompile supports being called with DELEGATECALL or CALLCODE?
	///
	/// If all checks return None, defaults to `false`.
	fn accept_delegate_call() -> Option<bool> {
		None
	}

	#[inline(always)]
	/// Is this precompile callable by a smart contract?
	///
	/// If all checks return None, defaults to `false`.
	fn callable_by_smart_contract(_caller: H160, _called_selector: Option<u32>) -> Option<bool> {
		None
	}

	#[inline(always)]
	/// Is this precompile callable by a precompile?
	///
	/// If all checks return None, defaults to `false`.
	fn callable_by_precompile(_caller: H160, _called_selector: Option<u32>) -> Option<bool> {
		None
	}

	#[inline(always)]
	/// Is this precompile able to do subcalls?
	///
	/// If all checks return None, defaults to `false`.
	fn allow_subcalls() -> Option<bool> {
		None
	}

	/// Summarize the checks when being called by a smart contract.
	fn callable_by_smart_contract_summary() -> Option<String> {
		None
	}

	/// Summarize the checks when being called by a precompile.
	fn callable_by_precompile_summary() -> Option<String> {
		None
	}
}

#[derive(Debug, Clone)]
pub enum DiscriminantResult<T> {
	Some(T, u64),
	None(u64),
	OutOfGas,
}

impl<T> From<DiscriminantResult<T>> for IsPrecompileResult {
	fn from(val: DiscriminantResult<T>) -> Self {
		match val {
			DiscriminantResult::<T>::Some(_, extra_cost) => IsPrecompileResult::Answer {
				is_precompile: true,
				extra_cost,
			},
			DiscriminantResult::<T>::None(extra_cost) => IsPrecompileResult::Answer {
				is_precompile: false,
				extra_cost,
			},
			DiscriminantResult::<T>::OutOfGas => IsPrecompileResult::OutOfGas,
		}
	}
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "testing", derive(serde::Serialize, serde::Deserialize))]
pub enum PrecompileKind {
	Single(H160),
	Prefixed(Vec<u8>),
	Multiple(Vec<H160>),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "testing", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecompileCheckSummary {
	pub name: Option<String>,
	pub precompile_kind: PrecompileKind,
	pub recursion_limit: Option<u16>,
	pub accept_delegate_call: bool,
	pub callable_by_smart_contract: String,
	pub callable_by_precompile: String,
}

#[impl_for_tuples(0, 20)]
impl PrecompileChecks for Tuple {
	#[inline(always)]
	fn recursion_limit() -> Option<Option<u16>> {
		for_tuples!(#(
			if let Some(check) = Tuple::recursion_limit() {
				return Some(check);
			}
		)*);

		None
	}

	#[inline(always)]
	fn accept_delegate_call() -> Option<bool> {
		for_tuples!(#(
			if let Some(check) = Tuple::accept_delegate_call() {
				return Some(check);
			}
		)*);

		None
	}

	#[inline(always)]
	fn callable_by_smart_contract(caller: H160, called_selector: Option<u32>) -> Option<bool> {
		for_tuples!(#(
			if let Some(check) = Tuple::callable_by_smart_contract(caller, called_selector) {
				return Some(check);
			}
		)*);

		None
	}

	#[inline(always)]
	fn callable_by_precompile(caller: H160, called_selector: Option<u32>) -> Option<bool> {
		for_tuples!(#(
			if let Some(check) = Tuple::callable_by_precompile(caller, called_selector) {
				return Some(check);
			}
		)*);

		None
	}

	#[inline(always)]
	fn allow_subcalls() -> Option<bool> {
		for_tuples!(#(
			if let Some(check) = Tuple::allow_subcalls() {
				return Some(check);
			}
		)*);

		None
	}

	fn callable_by_smart_contract_summary() -> Option<String> {
		for_tuples!(#(
			if let Some(check) = Tuple::callable_by_smart_contract_summary() {
				return Some(check);
			}
		)*);

		None
	}

	fn callable_by_precompile_summary() -> Option<String> {
		for_tuples!(#(
			if let Some(check) = Tuple::callable_by_precompile_summary() {
				return Some(check);
			}
		)*);

		None
	}
}

/// Precompile can be called using DELEGATECALL/CALLCODE.
pub struct AcceptDelegateCall;

impl PrecompileChecks for AcceptDelegateCall {
	#[inline(always)]
	fn accept_delegate_call() -> Option<bool> {
		Some(true)
	}
}

/// Precompile is able to do subcalls with provided nesting limit.
pub struct SubcallWithMaxNesting<const R: u16>;

impl<const R: u16> PrecompileChecks for SubcallWithMaxNesting<R> {
	#[inline(always)]
	fn recursion_limit() -> Option<Option<u16>> {
		Some(Some(R))
	}

	#[inline(always)]
	fn allow_subcalls() -> Option<bool> {
		Some(true)
	}
}

pub trait SelectorFilter {
	fn is_allowed(_caller: H160, _selector: Option<u32>) -> bool;

	fn description() -> String;
}
pub struct ForAllSelectors;
impl SelectorFilter for ForAllSelectors {
	fn is_allowed(_caller: H160, _selector: Option<u32>) -> bool {
		true
	}

	fn description() -> String {
		"Allowed for all selectors and callers".into()
	}
}

pub struct OnlyFrom<T>(PhantomData<T>);
impl<T: Get<H160>> SelectorFilter for OnlyFrom<T> {
	fn is_allowed(caller: H160, _selector: Option<u32>) -> bool {
		caller == T::get()
	}

	fn description() -> String {
		alloc::format!("Allowed for all selectors only if called from {}", T::get())
	}
}

pub struct CallableByContract<T = ForAllSelectors>(PhantomData<T>);

impl<T: SelectorFilter> PrecompileChecks for CallableByContract<T> {
	#[inline(always)]
	fn callable_by_smart_contract(caller: H160, called_selector: Option<u32>) -> Option<bool> {
		Some(T::is_allowed(caller, called_selector))
	}

	fn callable_by_smart_contract_summary() -> Option<String> {
		Some(T::description())
	}
}

/// Precompiles are allowed to call this precompile.
pub struct CallableByPrecompile<T = ForAllSelectors>(PhantomData<T>);

impl<T: SelectorFilter> PrecompileChecks for CallableByPrecompile<T> {
	#[inline(always)]
	fn callable_by_precompile(caller: H160, called_selector: Option<u32>) -> Option<bool> {
		Some(T::is_allowed(caller, called_selector))
	}

	fn callable_by_precompile_summary() -> Option<String> {
		Some(T::description())
	}
}

/// The type of EVM address.
#[derive(PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub enum AddressType {
	/// No code is stored at the address, therefore is EOA.
	EOA,
	/// The 5-byte magic constant for a precompile is stored at the address.
	Precompile,
	/// Every address that is not a EOA or a Precompile is potentially a Smart Contract.
	Contract,
}

/// Retrieves the type of address demarcated by `AddressType`.
pub fn get_address_type<R: pallet_evm::Config>(
	handle: &mut impl PrecompileHandle,
	address: H160,
) -> Result<AddressType, ExitError> {
	// Check if address is a precompile
	if let Ok(true) = is_precompile_or_fail::<R>(address, handle.remaining_gas()) {
		return Ok(AddressType::Precompile);
	}

	// Contracts under-construction don't have code yet
	if handle.is_contract_being_constructed(address) {
		return Ok(AddressType::Contract);
	}

	// AccountCodesMetadata:
	// Blake2128(16) + H160(20) + CodeMetadata(40)
	handle.record_db_read::<R>(ACCOUNT_CODES_METADATA_PROOF_SIZE as usize)?;
	let code_len = pallet_evm::Pallet::<R>::account_code_metadata(address).size;

	// Having no code at this point means that the address is an EOA
	if code_len == 0 {
		return Ok(AddressType::EOA);
	}

	Ok(AddressType::Contract)
}

/// Common checks for precompile and precompile sets.
/// Don't contain recursion check as precompile sets have recursion check for each member.
fn common_checks<R: pallet_evm::Config, C: PrecompileChecks>(
	handle: &mut impl PrecompileHandle,
) -> EvmResult<()> {
	let code_address = handle.code_address();
	let caller = handle.context().caller;

	// Check DELEGATECALL config.
	let accept_delegate_call = C::accept_delegate_call().unwrap_or(false);
	if !accept_delegate_call && code_address != handle.context().address {
		return Err(revert("Cannot be called with DELEGATECALL or CALLCODE"));
	}

	// Extract which selector is called.
	let selector = handle.input().get(0..4).map(|bytes| {
		let mut buffer = [0u8; 4];
		buffer.copy_from_slice(bytes);
		u32::from_be_bytes(buffer)
	});

	let caller_address_type = get_address_type::<R>(handle, caller)?;
	match caller_address_type {
		AddressType::Precompile => {
			// Is this selector callable from a precompile?
			let callable_by_precompile =
				C::callable_by_precompile(caller, selector).unwrap_or(false);
			if !callable_by_precompile {
				return Err(revert("Function not callable by precompiles"));
			}
		}
		AddressType::Contract => {
			// Is this selector callable from a smart contract?
			let callable_by_smart_contract =
				C::callable_by_smart_contract(caller, selector).unwrap_or(false);
			if !callable_by_smart_contract {
				return Err(revert("Function not callable by smart contracts"));
			}
		}
		AddressType::EOA => {
			// No check required for EOA
		}
	}

	Ok(())
}

pub fn is_precompile_or_fail<R: pallet_evm::Config>(address: H160, gas: u64) -> EvmResult<bool> {
	match <R as pallet_evm::Config>::PrecompilesValue::get().is_precompile(address, gas) {
		IsPrecompileResult::Answer { is_precompile, .. } => Ok(is_precompile),
		IsPrecompileResult::OutOfGas => Err(PrecompileFailure::Error {
			exit_status: ExitError::OutOfGas,
		}),
	}
}

pub struct AddressU64<const N: u64>;
impl<const N: u64> Get<H160> for AddressU64<N> {
	#[inline(always)]
	fn get() -> H160 {
		H160::from_low_u64_be(N)
	}
}

pub struct RestrictiveHandle<'a, H> {
	handle: &'a mut H,
	allow_subcalls: bool,
}

impl<'a, H: PrecompileHandle> PrecompileHandle for RestrictiveHandle<'a, H> {
	fn call(
		&mut self,
		address: H160,
		transfer: Option<evm::Transfer>,
		input: Vec<u8>,
		target_gas: Option<u64>,
		is_static: bool,
		context: &evm::Context,
	) -> (evm::ExitReason, Vec<u8>) {
		if !self.allow_subcalls {
			return (
				evm::ExitReason::Revert(evm::ExitRevert::Reverted),
				crate::solidity::revert::revert_as_bytes("subcalls disabled for this precompile"),
			);
		}

		self.handle
			.call(address, transfer, input, target_gas, is_static, context)
	}

	fn record_cost(&mut self, cost: u64) -> Result<(), evm::ExitError> {
		self.handle.record_cost(cost)
	}

	fn remaining_gas(&self) -> u64 {
		self.handle.remaining_gas()
	}

	fn log(
		&mut self,
		address: H160,
		topics: Vec<H256>,
		data: Vec<u8>,
	) -> Result<(), evm::ExitError> {
		self.handle.log(address, topics, data)
	}

	fn code_address(&self) -> H160 {
		self.handle.code_address()
	}

	fn input(&self) -> &[u8] {
		self.handle.input()
	}

	fn context(&self) -> &evm::Context {
		self.handle.context()
	}

	fn origin(&self) -> H160 {
		self.handle.origin()
	}

	fn is_static(&self) -> bool {
		self.handle.is_static()
	}

	fn gas_limit(&self) -> Option<u64> {
		self.handle.gas_limit()
	}

	fn record_external_cost(
		&mut self,
		ref_time: Option<u64>,
		proof_size: Option<u64>,
		storage_growth: Option<u64>,
	) -> Result<(), ExitError> {
		self.handle
			.record_external_cost(ref_time, proof_size, storage_growth)
	}

	fn refund_external_cost(&mut self, ref_time: Option<u64>, proof_size: Option<u64>) {
		self.handle.refund_external_cost(ref_time, proof_size)
	}

	fn is_contract_being_constructed(&self, address: H160) -> bool {
		self.handle.is_contract_being_constructed(address)
	}
}

/// Allows to know if a precompile is active or not.
/// This allows to detect deactivated precompile, that are still considered precompiles by
/// the EVM but that will always revert when called.
pub trait IsActivePrecompile {
	/// Is the provided address an active precompile, a precompile that has
	/// not be deactivated. Note that a deactivated precompile is still considered a precompile
	/// for the EVM, but it will always revert when called.
	fn is_active_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult;
}

// INDIVIDUAL PRECOMPILE(SET)

/// A fragment of a PrecompileSet. Should be implemented as is it
/// was a PrecompileSet containing only the precompile(set) it wraps.
/// They can be combined into a real PrecompileSet using `PrecompileSetBuilder`.
pub trait PrecompileSetFragment {
	/// Instanciate the fragment.
	fn new() -> Self;

	/// Execute the fragment.
	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult>;

	/// Is the provided address a precompile in this fragment?
	fn is_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult;

	/// Return the list of addresses covered by this fragment.
	fn used_addresses(&self) -> Vec<H160>;

	/// Summarize
	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary>;
}

/// Wraps a stateless precompile: a type implementing the `Precompile` trait.
/// Type parameters allow to define:
/// - A: The address of the precompile
/// - R: The recursion limit (defaults to 1)
/// - D: If DELEGATECALL is supported (default to no)
pub struct PrecompileAt<A, P, C = ()> {
	current_recursion_level: RefCell<u16>,
	_phantom: PhantomData<(A, P, C)>,
}

impl<A, P, C> PrecompileSetFragment for PrecompileAt<A, P, C>
where
	A: Get<H160>,
	P: Precompile,
	C: PrecompileChecks,
{
	#[inline(always)]
	fn new() -> Self {
		Self {
			current_recursion_level: RefCell::new(0),
			_phantom: PhantomData,
		}
	}

	#[inline(always)]
	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult> {
		let code_address = handle.code_address();

		// Check if this is the address of the precompile.
		if A::get() != code_address {
			return None;
		}

		// Perform common checks.
		if let Err(err) = common_checks::<R, C>(handle) {
			return Some(Err(err));
		}

		// Check and increase recursion level if needed.
		let recursion_limit = C::recursion_limit().unwrap_or(Some(0));
		if let Some(max_recursion_level) = recursion_limit {
			match self.current_recursion_level.try_borrow_mut() {
				Ok(mut recursion_level) => {
					if *recursion_level > max_recursion_level {
						return Some(Err(revert("Precompile is called with too high nesting")));
					}

					*recursion_level += 1;
				}
				// We don't hold the borrow and are in single-threaded code, thus we should
				// not be able to fail borrowing in nested calls.
				Err(_) => return Some(Err(revert("Couldn't check precompile nesting"))),
			}
		}

		// Subcall protection.
		let allow_subcalls = C::allow_subcalls().unwrap_or(false);
		let mut handle = RestrictiveHandle {
			handle,
			allow_subcalls,
		};

		let res = P::execute(&mut handle);

		// Decrease recursion level if needed.
		if recursion_limit.is_some() {
			match self.current_recursion_level.try_borrow_mut() {
				Ok(mut recursion_level) => {
					*recursion_level -= 1;
				}
				// We don't hold the borrow and are in single-threaded code, thus we should
				// not be able to fail borrowing in nested calls.
				Err(_) => return Some(Err(revert("Couldn't check precompile nesting"))),
			}
		}

		Some(res)
	}

	#[inline(always)]
	fn is_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: address == A::get(),
			extra_cost: 0,
		}
	}

	#[inline(always)]
	fn used_addresses(&self) -> Vec<H160> {
		vec![A::get()]
	}

	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		vec![PrecompileCheckSummary {
			name: None,
			precompile_kind: PrecompileKind::Single(A::get()),
			recursion_limit: C::recursion_limit().unwrap_or(Some(0)),
			accept_delegate_call: C::accept_delegate_call().unwrap_or(false),
			callable_by_smart_contract: C::callable_by_smart_contract_summary()
				.unwrap_or_else(|| "Not callable".into()),
			callable_by_precompile: C::callable_by_precompile_summary()
				.unwrap_or_else(|| "Not callable".into()),
		}]
	}
}

impl<A, P, C> IsActivePrecompile for PrecompileAt<A, P, C>
where
	A: Get<H160>,
{
	#[inline(always)]
	fn is_active_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: address == A::get(),
			extra_cost: 0,
		}
	}
}

/// Wraps an inner PrecompileSet with all its addresses starting with
/// a common prefix.
/// Type parameters allow to define:
/// - A: The common prefix
/// - D: If DELEGATECALL is supported (default to no)
pub struct PrecompileSetStartingWith<A, P, C = ()> {
	precompile_set: P,
	current_recursion_level: RefCell<BTreeMap<H160, u16>>,
	_phantom: PhantomData<(A, C)>,
}

impl<A, P, C> PrecompileSetFragment for PrecompileSetStartingWith<A, P, C>
where
	A: Get<&'static [u8]>,
	P: PrecompileSet + Default,
	C: PrecompileChecks,
{
	#[inline(always)]
	fn new() -> Self {
		Self {
			precompile_set: P::default(),
			current_recursion_level: RefCell::new(BTreeMap::new()),
			_phantom: PhantomData,
		}
	}

	#[inline(always)]
	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult> {
		let code_address = handle.code_address();
		if !is_precompile_or_fail::<R>(code_address, handle.remaining_gas()).ok()? {
			return None;
		}
		// Perform common checks.
		if let Err(err) = common_checks::<R, C>(handle) {
			return Some(Err(err));
		}

		// Check and increase recursion level if needed.
		let recursion_limit = C::recursion_limit().unwrap_or(Some(0));
		if let Some(max_recursion_level) = recursion_limit {
			match self.current_recursion_level.try_borrow_mut() {
				Ok(mut recursion_level_map) => {
					let recursion_level = recursion_level_map.entry(code_address).or_insert(0);

					if *recursion_level > max_recursion_level {
						return Some(Err(revert("Precompile is called with too high nesting")));
					}

					*recursion_level += 1;
				}
				// We don't hold the borrow and are in single-threaded code, thus we should
				// not be able to fail borrowing in nested calls.
				Err(_) => return Some(Err(revert("Couldn't check precompile nesting"))),
			}
		}

		// Subcall protection.
		let allow_subcalls = C::allow_subcalls().unwrap_or(false);
		let mut handle = RestrictiveHandle {
			handle,
			allow_subcalls,
		};

		let res = self.precompile_set.execute(&mut handle);

		// Decrease recursion level if needed.
		if recursion_limit.is_some() {
			match self.current_recursion_level.try_borrow_mut() {
				Ok(mut recursion_level_map) => {
					let recursion_level = match recursion_level_map.get_mut(&code_address) {
						Some(recursion_level) => recursion_level,
						None => return Some(Err(revert("Couldn't retrieve precompile nesting"))),
					};

					*recursion_level -= 1;
				}
				// We don't hold the borrow and are in single-threaded code, thus we should
				// not be able to fail borrowing in nested calls.
				Err(_) => return Some(Err(revert("Couldn't check precompile nesting"))),
			}
		}

		res
	}

	#[inline(always)]
	fn is_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		if address.as_bytes().starts_with(A::get()) {
			return self.precompile_set.is_precompile(address, gas);
		}
		IsPrecompileResult::Answer {
			is_precompile: false,
			extra_cost: 0,
		}
	}

	#[inline(always)]
	fn used_addresses(&self) -> Vec<H160> {
		// TODO: We currently can't get the list of used addresses.
		vec![]
	}

	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		let prefix = A::get();

		vec![PrecompileCheckSummary {
			name: None,
			precompile_kind: PrecompileKind::Prefixed(prefix.to_vec()),
			recursion_limit: C::recursion_limit().unwrap_or(Some(0)),
			accept_delegate_call: C::accept_delegate_call().unwrap_or(false),
			callable_by_smart_contract: C::callable_by_smart_contract_summary()
				.unwrap_or_else(|| "Not callable".into()),
			callable_by_precompile: C::callable_by_precompile_summary()
				.unwrap_or_else(|| "Not callable".into()),
		}]
	}
}

impl<A, P, C> IsActivePrecompile for PrecompileSetStartingWith<A, P, C>
where
	Self: PrecompileSetFragment,
{
	#[inline(always)]
	fn is_active_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		self.is_precompile(address, gas)
	}
}

/// Make a precompile that always revert.
/// Can be useful when writing tests.
pub struct RevertPrecompile<A>(PhantomData<A>);

impl<A> PrecompileSetFragment for RevertPrecompile<A>
where
	A: Get<H160>,
{
	#[inline(always)]
	fn new() -> Self {
		Self(PhantomData)
	}

	#[inline(always)]
	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult> {
		if A::get() == handle.code_address() {
			Some(Err(revert("revert")))
		} else {
			None
		}
	}

	#[inline(always)]
	fn is_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: address == A::get(),
			extra_cost: 0,
		}
	}

	#[inline(always)]
	fn used_addresses(&self) -> Vec<H160> {
		vec![A::get()]
	}

	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		vec![PrecompileCheckSummary {
			name: None,
			precompile_kind: PrecompileKind::Single(A::get()),
			recursion_limit: Some(0),
			accept_delegate_call: true,
			callable_by_smart_contract: "Reverts in all cases".into(),
			callable_by_precompile: "Reverts in all cases".into(),
		}]
	}
}

impl<A> IsActivePrecompile for RevertPrecompile<A> {
	#[inline(always)]
	fn is_active_precompile(&self, _address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: true,
			extra_cost: 0,
		}
	}
}

/// Precompiles that were removed from a precompile set.
/// Still considered precompiles but are inactive and always revert.
pub struct RemovedPrecompilesAt<A>(PhantomData<A>);
impl<A> PrecompileSetFragment for RemovedPrecompilesAt<A>
where
	A: Get<Vec<H160>>,
{
	#[inline(always)]
	fn new() -> Self {
		Self(PhantomData)
	}

	#[inline(always)]
	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult> {
		if A::get().contains(&handle.code_address()) {
			Some(Err(revert("Removed precompile")))
		} else {
			None
		}
	}

	#[inline(always)]
	fn is_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: A::get().contains(&address),
			extra_cost: 0,
		}
	}

	#[inline(always)]
	fn used_addresses(&self) -> Vec<H160> {
		A::get()
	}

	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		vec![PrecompileCheckSummary {
			name: None,
			precompile_kind: PrecompileKind::Multiple(A::get()),
			recursion_limit: Some(0),
			accept_delegate_call: true,
			callable_by_smart_contract: "Reverts in all cases".into(),
			callable_by_precompile: "Reverts in all cases".into(),
		}]
	}
}

impl<A> IsActivePrecompile for RemovedPrecompilesAt<A>
where
	Self: PrecompileSetFragment,
{
	#[inline(always)]
	fn is_active_precompile(&self, _address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: false,
			extra_cost: 0,
		}
	}
}

/// A precompile that was removed from a precompile set.
/// Still considered a precompile but is inactive and always revert.
pub struct RemovedPrecompileAt<A>(PhantomData<A>);
impl<A> PrecompileSetFragment for RemovedPrecompileAt<A>
where
	A: Get<H160>,
{
	#[inline(always)]
	fn new() -> Self {
		Self(PhantomData)
	}

	#[inline(always)]
	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult> {
		if A::get() == handle.code_address() {
			Some(Err(revert("Removed precompile")))
		} else {
			None
		}
	}

	#[inline(always)]
	fn is_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: address == A::get(),
			extra_cost: 0,
		}
	}

	#[inline(always)]
	fn used_addresses(&self) -> Vec<H160> {
		vec![A::get()]
	}

	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		vec![PrecompileCheckSummary {
			name: None,
			precompile_kind: PrecompileKind::Single(A::get()),
			recursion_limit: Some(0),
			accept_delegate_call: true,
			callable_by_smart_contract: "Reverts in all cases".into(),
			callable_by_precompile: "Reverts in all cases".into(),
		}]
	}
}

impl<A> IsActivePrecompile for RemovedPrecompileAt<A> {
	#[inline(always)]
	fn is_active_precompile(&self, _address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: false,
			extra_cost: 0,
		}
	}
}

// COMPOSITION OF PARTS
#[impl_for_tuples(1, 100)]
impl PrecompileSetFragment for Tuple {
	#[inline(always)]
	fn new() -> Self {
		(for_tuples!(#(
			Tuple::new()
		),*))
	}

	#[inline(always)]
	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult> {
		for_tuples!(#(
			if let Some(res) = self.Tuple.execute::<R>(handle) {
				return Some(res);
			}
		)*);

		None
	}

	#[inline(always)]
	fn is_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		for_tuples!(#(
			if let IsPrecompileResult::Answer {
			is_precompile: true,
				..
			} = self.Tuple.is_precompile(address, gas) { return IsPrecompileResult::Answer {
				is_precompile: true,
				extra_cost: 0,
				}
			};
		)*);
		IsPrecompileResult::Answer {
			is_precompile: false,
			extra_cost: 0,
		}
	}

	#[inline(always)]
	fn used_addresses(&self) -> Vec<H160> {
		let mut used_addresses = vec![];

		for_tuples!(#(
			let mut inner = self.Tuple.used_addresses();
			used_addresses.append(&mut inner);
		)*);

		used_addresses
	}

	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		let mut checks = Vec::new();

		for_tuples!(#(
			let mut inner = self.Tuple.summarize_checks();
			checks.append(&mut inner);
		)*);

		checks
	}
}

#[impl_for_tuples(1, 100)]
impl IsActivePrecompile for Tuple {
	#[inline(always)]
	fn is_active_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		for_tuples!(#(
			if let IsPrecompileResult::Answer {
				is_precompile: true,
				..
			} = self.Tuple.is_active_precompile(address, gas) { return IsPrecompileResult::Answer {
				is_precompile: true,
				extra_cost: 0,
			} };
		)*);
		IsPrecompileResult::Answer {
			is_precompile: false,
			extra_cost: 0,
		}
	}
}

/// Wraps a precompileset fragment into a range, and will skip processing it if the address
/// is out of the range.
pub struct PrecompilesInRangeInclusive<R, P> {
	inner: P,
	range: RangeInclusive<H160>,
	_phantom: PhantomData<R>,
}

impl<S, E, P> PrecompileSetFragment for PrecompilesInRangeInclusive<(S, E), P>
where
	S: Get<H160>,
	E: Get<H160>,
	P: PrecompileSetFragment,
{
	fn new() -> Self {
		Self {
			inner: P::new(),
			range: RangeInclusive::new(S::get(), E::get()),
			_phantom: PhantomData,
		}
	}

	fn execute<R: pallet_evm::Config>(
		&self,
		handle: &mut impl PrecompileHandle,
	) -> Option<PrecompileResult> {
		if self.range.contains(&handle.code_address()) {
			self.inner.execute::<R>(handle)
		} else {
			None
		}
	}

	fn is_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		if self.range.contains(&address) {
			self.inner.is_precompile(address, gas)
		} else {
			IsPrecompileResult::Answer {
				is_precompile: false,
				extra_cost: 0,
			}
		}
	}

	fn used_addresses(&self) -> Vec<H160> {
		self.inner.used_addresses()
	}

	fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		self.inner.summarize_checks()
	}
}

impl<S, E, P> IsActivePrecompile for PrecompilesInRangeInclusive<(S, E), P>
where
	P: IsActivePrecompile,
{
	fn is_active_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		if self.range.contains(&address) {
			self.inner.is_active_precompile(address, gas)
		} else {
			IsPrecompileResult::Answer {
				is_precompile: false,
				extra_cost: 0,
			}
		}
	}
}

/// Wraps a tuple of `PrecompileSetFragment` to make a real `PrecompileSet`.
pub struct PrecompileSetBuilder<R, P> {
	inner: P,
	_phantom: PhantomData<R>,
}

impl<R: pallet_evm::Config, P: PrecompileSetFragment> PrecompileSet for PrecompileSetBuilder<R, P> {
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		self.inner.execute::<R>(handle)
	}

	fn is_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		self.inner.is_precompile(address, gas)
	}
}

impl<R, P: IsActivePrecompile> IsActivePrecompile for PrecompileSetBuilder<R, P> {
	fn is_active_precompile(&self, address: H160, gas: u64) -> IsPrecompileResult {
		self.inner.is_active_precompile(address, gas)
	}
}

impl<R: pallet_evm::Config, P: PrecompileSetFragment> Default for PrecompileSetBuilder<R, P> {
	fn default() -> Self {
		Self::new()
	}
}

impl<R: pallet_evm::Config, P: PrecompileSetFragment> PrecompileSetBuilder<R, P> {
	/// Create a new instance of the PrecompileSet.
	pub fn new() -> Self {
		Self {
			inner: P::new(),
			_phantom: PhantomData,
		}
	}

	/// Return the list of mapped addresses contained in this PrecompileSet.
	pub fn used_addresses() -> impl Iterator<Item = pallet_evm::AccountIdOf<R>> {
		Self::used_addresses_h160().map(R::AddressMapping::into_account_id)
	}

	/// Return the list of H160 addresses contained in this PrecompileSet.
	pub fn used_addresses_h160() -> impl Iterator<Item = H160> {
		Self::new().inner.used_addresses().into_iter()
	}

	pub fn summarize_checks(&self) -> Vec<PrecompileCheckSummary> {
		self.inner.summarize_checks()
	}
}
