// This file is part of Frontier.

// Copyright (C) Frontier developers.
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

mod runtime;

use crate::{Config, WeightInfo, ConvertPolkaVmGas};
use fp_evm::PrecompileHandle;
use sp_runtime::Weight;

pub use self::runtime::{ExecResult, Runtime, RuntimeCosts, SupervisorError};

pub const PREFIX: [u8; 8] = [0xef, 0x70, 0x6F, 0x6C, 0x6B, 0x61, 0x76, 0x6D];
pub const CALL_IDENTIFIER: &str = "call";
pub const PAGE_SIZE: u32 = 4 * 1024;
pub const SENTINEL: u32 = u32::MAX;
pub const LOG_TARGET: &str = "runtime::evm::polkavm";

fn code_load_weight<T: Config>(size: u32) -> Weight {
	<T as Config>::WeightInfo::call_with_code_per_byte(size)
}

pub struct PreparedCall<'a, T, H> {
	module: polkavm::Module,
	instance: polkavm::RawInstance,
	runtime: Runtime<'a, T, H, polkavm::RawInstance>,
}

impl<'a, T: Config, H: PrecompileHandle> PreparedCall<'a, T, H> {
	pub fn load(handle: &'a mut H) -> Result<Self, SupervisorError> {
		let code = pallet_evm::AccountCodes::<T>::get(handle.code_address());
		if code[0..8] != PREFIX {
			return Err(SupervisorError::NotPolkaVm);
		}
		let code_load_weight = code_load_weight::<T>(code.len() as u32);
		handle.record_external_cost(Some(code_load_weight.ref_time()), Some(code_load_weight.proof_size()), None).map_err(|_| SupervisorError::OutOfGas)?;

		let polkavm_code = &code[8..];

		let mut config = polkavm::Config::default();
		config.set_backend(Some(polkavm::BackendKind::Interpreter));
		config.set_cache_enabled(false);

		let engine = polkavm::Engine::new(&config).expect(
			"on-chain (no_std) use of interpreter is hard coded.
				interpreter is available on all platforms; qed",
		);

		let mut module_config = polkavm::ModuleConfig::new();
		module_config.set_page_size(PAGE_SIZE);
		module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
		module_config.set_allow_sbrk(false);
		let module = polkavm::Module::new(&engine, &module_config, polkavm_code.into())
			.map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create polkavm module: {err:?}");
			SupervisorError::CodeRejected
		})?;

		let entry_program_counter = module
			.exports()
			.find(|export| export.symbol().as_bytes() == CALL_IDENTIFIER.as_bytes())
			.ok_or_else(|| SupervisorError::CodeRejected)?
			.program_counter();
		let input_data = handle.input().to_vec();
		let gas_limit_polkavm = T::ConvertPolkaVmGas::evm_gas_to_polkavm_gas(handle.gas_limit().ok_or(SupervisorError::OutOfGas)?);
		let runtime: Runtime<'_, T, _, polkavm::RawInstance> = Runtime::new(handle, input_data, gas_limit_polkavm);

		let mut instance = module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to instantiate polkavm module: {err:?}");
			SupervisorError::CodeRejected
		})?;

		instance.set_gas(gas_limit_polkavm);
		instance.prepare_call_untyped(entry_program_counter, &[]);

		Ok(Self {
			module,
			instance,
			runtime,
		})
	}

	pub fn call(mut self) -> ExecResult {
		let exec_result = loop {
			let interrupt = self.instance.run();
			if let Some(exec_result) =
				self.runtime
					.handle_interrupt(interrupt, &self.module, &mut self.instance)
			{
				break exec_result;
			}
		};
		self.runtime.charge_polkavm_gas(&mut self.instance)?;
		exec_result
	}

	/// The guest memory address at which the aux data is located.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn aux_data_base(&self) -> u32 {
		self.instance.module().memory_map().aux_data_address()
	}

	/// Copies `data` to the aux data at address `offset`.
	///
	/// It sets `a0` to the beginning of data inside the aux data.
	/// It sets `a1` to the value passed.
	///
	/// Only used in benchmarking so far.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn setup_aux_data(&mut self, data: &[u8], offset: u32, a1: u64) -> DispatchResult {
		let a0 = self.aux_data_base().saturating_add(offset);
		self.instance.write_memory(a0, data).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to write aux data: {err:?}");
			Error::<E::T>::CodeRejected
		})?;
		self.instance.set_reg(polkavm::Reg::A0, a0.into());
		self.instance.set_reg(polkavm::Reg::A1, a1);
		Ok(())
	}
}
