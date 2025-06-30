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

use crate::Config;
use fp_evm::PrecompileHandle;

pub use self::runtime::{ExecResult, Runtime, RuntimeCosts};

pub struct PreparedCall<'a, T, H> {
	module: polkavm::Module,
	instance: polkavm::RawInstance,
	runtime: Runtime<'a, T, H, polkavm::RawInstance>,
}

impl<'a, T: Config, H: PrecompileHandle> PreparedCall<'a, T, H> {
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
