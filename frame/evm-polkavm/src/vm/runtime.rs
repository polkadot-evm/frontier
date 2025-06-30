// This file is part of Frontier.

// Copyright (C) Frontier developers.
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

//! Environment definition of the vm smart-contract runtime.

use alloc::{vec, vec::Vec};
use core::{fmt, marker::PhantomData};
use fp_evm::PrecompileHandle;
use frame_support::weights::Weight;
use pallet_evm_polkavm_proc_macro::define_env;
use pallet_evm_polkavm_uapi::{ReturnErrorCode, ReturnFlags};
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{H160, H256, U256};
use sp_runtime::RuntimeDebug;

use crate::{Config, WeightInfo};

const SENTINEL: u32 = u32::MAX;
const LOG_TARGET: &str = "runtime::evm::polkavm";

/// Output of a contract call or instantiation which ran to completion.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo, Default)]
pub struct ExecReturnValue {
	/// Flags passed along by `seal_return`. Empty when `seal_return` was never called.
	pub flags: ReturnFlags,
	/// Buffer passed along by `seal_return`. Empty when `seal_return` was never called.
	pub data: Vec<u8>,
}

pub type ExecResult = Result<ExecReturnValue, SupervisorError>;

impl ExecReturnValue {
	/// The contract did revert all storage changes.
	pub fn did_revert(&self) -> bool {
		self.flags.contains(ReturnFlags::REVERT)
	}
}

/// Abstraction over the memory access within syscalls.
///
/// The reason for this abstraction is that we run syscalls on the host machine when
/// benchmarking them. In that case we have direct access to the contract's memory. However, when
/// running within PolkaVM we need to resort to copying as we can't map the contracts memory into
/// the host (as of now).
pub trait Memory {
	/// Read designated chunk from the sandbox memory into the supplied buffer.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - requested buffer is not within the bounds of the sandbox memory.
	fn read_into_buf(&self, ptr: u32, buf: &mut [u8]) -> Result<(), SupervisorError>;

	/// Write the given buffer to the designated location in the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - designated area is not within the bounds of the sandbox memory.
	fn write(&mut self, ptr: u32, buf: &[u8]) -> Result<(), SupervisorError>;

	/// Zero the designated location in the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - designated area is not within the bounds of the sandbox memory.
	fn zero(&mut self, ptr: u32, len: u32) -> Result<(), SupervisorError>;

	/// Read designated chunk from the sandbox memory.
	///
	/// Returns `Err` if one of the following conditions occurs:
	///
	/// - requested buffer is not within the bounds of the sandbox memory.
	fn read(&self, ptr: u32, len: u32) -> Result<Vec<u8>, SupervisorError> {
		let mut buf = vec![0u8; len as usize];
		self.read_into_buf(ptr, buf.as_mut_slice())?;
		Ok(buf)
	}

	/// Same as `read` but reads into a fixed size buffer.
	fn read_array<const N: usize>(&self, ptr: u32) -> Result<[u8; N], SupervisorError> {
		let mut buf = [0u8; N];
		self.read_into_buf(ptr, &mut buf)?;
		Ok(buf)
	}

	/// Read a `u32` from the sandbox memory.
	fn read_u32(&self, ptr: u32) -> Result<u32, SupervisorError> {
		let buf: [u8; 4] = self.read_array(ptr)?;
		Ok(u32::from_le_bytes(buf))
	}

	/// Read a `U256` from the sandbox memory.
	fn read_u256(&self, ptr: u32) -> Result<U256, SupervisorError> {
		let buf: [u8; 32] = self.read_array(ptr)?;
		Ok(U256::from_little_endian(&buf))
	}

	/// Read a `H160` from the sandbox memory.
	fn read_h160(&self, ptr: u32) -> Result<H160, SupervisorError> {
		let mut buf = H160::default();
		self.read_into_buf(ptr, buf.as_bytes_mut())?;
		Ok(buf)
	}

	/// Read a `H256` from the sandbox memory.
	fn read_h256(&self, ptr: u32) -> Result<H256, SupervisorError> {
		let mut code_hash = H256::default();
		self.read_into_buf(ptr, code_hash.as_bytes_mut())?;
		Ok(code_hash)
	}
}

/// Allows syscalls access to the PolkaVM instance they are executing in.
///
/// In case a contract is executing within PolkaVM its `memory` argument will also implement
/// this trait. The benchmarking implementation of syscalls will only require `Memory`
/// to be implemented.
pub trait PolkaVmInstance: Memory {
	fn gas(&self) -> polkavm::Gas;
	fn set_gas(&mut self, gas: polkavm::Gas);
	fn read_input_regs(&self) -> (u64, u64, u64, u64, u64, u64);
	fn write_output(&mut self, output: u64);
}

// Memory implementation used in benchmarking where guest memory is mapped into the host.
//
// Please note that we could optimize the `read_as_*` functions by decoding directly from
// memory without a copy. However, we don't do that because as it would change the behaviour
// of those functions: A `read_as` with a `len` larger than the actual type can succeed
// in the streaming implementation while it could fail with a segfault in the copy implementation.
#[cfg(feature = "runtime-benchmarks")]
impl Memory for [u8] {
	fn read_into_buf(&self, ptr: u32, buf: &mut [u8]) -> Result<(), SupervisorError> {
		let ptr = ptr as usize;
		let bound_checked = self
			.get(ptr..ptr + buf.len())
			.ok_or_else(|| SupervisorError::OutOfBounds)?;
		buf.copy_from_slice(bound_checked);
		Ok(())
	}

	fn write(&mut self, ptr: u32, buf: &[u8]) -> Result<(), SupervisorError> {
		let ptr = ptr as usize;
		let bound_checked = self
			.get_mut(ptr..ptr + buf.len())
			.ok_or_else(|| SupervisorError::OutOfBounds)?;
		bound_checked.copy_from_slice(buf);
		Ok(())
	}

	fn zero(&mut self, ptr: u32, len: u32) -> Result<(), SupervisorError> {
		<[u8] as Memory>::write(self, ptr, &vec![0; len as usize])
	}
}

impl Memory for polkavm::RawInstance {
	fn read_into_buf(&self, ptr: u32, buf: &mut [u8]) -> Result<(), SupervisorError> {
		self.read_memory_into(ptr, buf)
			.map(|_| ())
			.map_err(|_| SupervisorError::OutOfBounds.into())
	}

	fn write(&mut self, ptr: u32, buf: &[u8]) -> Result<(), SupervisorError> {
		self.write_memory(ptr, buf)
			.map_err(|_| SupervisorError::OutOfBounds.into())
	}

	fn zero(&mut self, ptr: u32, len: u32) -> Result<(), SupervisorError> {
		self.zero_memory(ptr, len)
			.map_err(|_| SupervisorError::OutOfBounds.into())
	}
}

impl PolkaVmInstance for polkavm::RawInstance {
	fn gas(&self) -> polkavm::Gas {
		self.gas()
	}

	fn set_gas(&mut self, gas: polkavm::Gas) {
		self.set_gas(gas)
	}

	fn read_input_regs(&self) -> (u64, u64, u64, u64, u64, u64) {
		(
			self.reg(polkavm::Reg::A0),
			self.reg(polkavm::Reg::A1),
			self.reg(polkavm::Reg::A2),
			self.reg(polkavm::Reg::A3),
			self.reg(polkavm::Reg::A4),
			self.reg(polkavm::Reg::A5),
		)
	}

	fn write_output(&mut self, output: u64) {
		self.set_reg(polkavm::Reg::A0, output);
	}
}

impl From<&ExecReturnValue> for ReturnErrorCode {
	fn from(from: &ExecReturnValue) -> Self {
		if from.flags.contains(ReturnFlags::REVERT) {
			Self::CalleeReverted
		} else {
			Self::Success
		}
	}
}

/// The data passed through when a contract uses `seal_return`.
#[derive(RuntimeDebug)]
pub struct ReturnData {
	/// The flags as passed through by the contract. They are still unchecked and
	/// will later be parsed into a `ReturnFlags` bitflags struct.
	flags: u32,
	/// The output buffer passed by the contract as return data.
	data: Vec<u8>,
}

#[derive(RuntimeDebug)]
pub enum SupervisorError {
	OutOfBounds,
	ExecutionFailed,
	ContractTrapped,
	OutOfGas,
	InvalidSyscall,
	InvalidCallFlags,
	StateChangeDenied,
	InputForwarded,
}

/// Enumerates all possible reasons why a trap was generated.
///
/// This is either used to supply the caller with more information about why an error
/// occurred (the SupervisorError variant).
/// The other case is where the trap does not constitute an error but rather was invoked
/// as a quick way to terminate the application (all other variants).
#[derive(RuntimeDebug)]
pub enum TrapReason {
	/// The supervisor trapped the contract because of an error condition occurred during
	/// execution in privileged code.
	SupervisorError(SupervisorError),
	/// Signals that trap was generated in response to call `seal_return` host function.
	Return(ReturnData),
}

impl From<SupervisorError> for TrapReason {
	fn from(from: SupervisorError) -> Self {
		TrapReason::SupervisorError(from)
	}
}

impl fmt::Display for TrapReason {
	fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
		Ok(())
	}
}

macro_rules! cost_args {
	// cost_args!(name, a, b, c) -> T::WeightInfo::name(a, b, c).saturating_sub(T::WeightInfo::name(0, 0, 0))
	($name:ident, $( $arg: expr ),+) => {
		(<T as Config>::WeightInfo::$name($( $arg ),+).saturating_sub(cost_args!(@call_zero $name, $( $arg ),+)))
	};
	// Transform T::WeightInfo::name(a, b, c) into T::WeightInfo::name(0, 0, 0)
	(@call_zero $name:ident, $( $arg:expr ),*) => {
		<T as Config>::WeightInfo::$name($( cost_args!(@replace_token $arg) ),*)
	};
	// Replace the token with 0.
	(@replace_token $_in:tt) => { 0 };
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Copy, Clone)]
pub enum RuntimeCosts {
	/// Base Weight of calling a host function.
	HostFn,
	/// Weight charged for copying data from the sandbox.
	CopyFromContract(u32),
	/// Weight of calling `seal_call_data_load``.
	CallDataLoad,
	/// Weight of calling `seal_call_data_copy`.
	CallDataCopy(u32),
	/// Weight of calling `seal_caller`.
	Caller,
	/// Weight of calling `seal_call_data_size`.
	CallDataSize,
	/// Weight of calling `seal_origin`.
	Origin,
	/// Weight of calling `seal_address`.
	Address,
	/// Weight of calling `seal_deposit_event` with the given number of topics and event size.
	DepositEvent { num_topic: u32, len: u32 },
}

impl RuntimeCosts {
	fn weight<T: Config>(&self) -> Weight {
		use self::RuntimeCosts::*;
		match *self {
			HostFn => cost_args!(noop_host_fn, 1),
			CopyFromContract(len) => <T as Config>::WeightInfo::seal_return(len),
			CallDataSize => <T as Config>::WeightInfo::seal_call_data_size(),
			CallDataLoad => <T as Config>::WeightInfo::seal_call_data_load(),
			CallDataCopy(len) => <T as Config>::WeightInfo::seal_call_data_copy(len),
			Caller => <T as Config>::WeightInfo::seal_caller(),
			Origin => <T as Config>::WeightInfo::seal_origin(),
			Address => <T as Config>::WeightInfo::seal_address(),
			DepositEvent { num_topic, len } => {
				<T as Config>::WeightInfo::seal_deposit_event(num_topic, len)
			}
		}
	}
}

/// This is only appropriate when writing out data of constant size that does not depend on user
/// input. In this case the costs for this copy was already charged as part of the token at
/// the beginning of the API entry point.
fn already_charged(_: u32) -> Option<RuntimeCosts> {
	None
}

/// Can only be used for one call.
pub struct Runtime<'a, T, H, M: ?Sized> {
	handle: &'a mut H,
	input_data: Option<Vec<u8>>,
	last_gas: polkavm::Gas,
	_phantom_data: PhantomData<(T, M)>,
}

impl<'a, T: Config, H: PrecompileHandle, M: PolkaVmInstance> Runtime<'a, T, H, M> {
	pub fn handle_interrupt(
		&mut self,
		interrupt: Result<polkavm::InterruptKind, polkavm::Error>,
		module: &polkavm::Module,
		instance: &mut M,
	) -> Option<ExecResult> {
		use polkavm::InterruptKind::*;

		match interrupt {
			Err(error) => {
				// in contrast to the other returns this "should" not happen: log level error
				log::error!(target: LOG_TARGET, "polkavm execution error: {error}");
				Some(Err(SupervisorError::ExecutionFailed.into()))
			}
			Ok(Finished) => Some(Ok(ExecReturnValue {
				flags: ReturnFlags::empty(),
				data: Vec::new(),
			})),
			Ok(Trap) => Some(Err(SupervisorError::ContractTrapped.into())),
			Ok(Segfault(_)) => Some(Err(SupervisorError::ExecutionFailed.into())),
			Ok(NotEnoughGas) => Some(Err(SupervisorError::OutOfGas.into())),
			Ok(Step) => None,
			Ok(Ecalli(idx)) => {
				// This is a special hard coded syscall index which is used by benchmarks
				// to abort contract execution. It is used to terminate the execution without
				// breaking up a basic block. The fixed index is used so that the benchmarks
				// don't have to deal with import tables.
				if cfg!(feature = "runtime-benchmarks") && idx == SENTINEL {
					return Some(Ok(ExecReturnValue {
						flags: ReturnFlags::empty(),
						data: Vec::new(),
					}));
				}
				let Some(syscall_symbol) = module.imports().get(idx) else {
					return Some(Err(SupervisorError::InvalidSyscall.into()));
				};
				match self.handle_ecall(instance, syscall_symbol.as_bytes()) {
					Ok(None) => None,
					Ok(Some(return_value)) => {
						instance.write_output(return_value);
						None
					}
					Err(TrapReason::Return(ReturnData { flags, data })) => {
						match ReturnFlags::from_bits(flags) {
							None => Some(Err(SupervisorError::InvalidCallFlags.into())),
							Some(flags) => Some(Ok(ExecReturnValue { flags, data })),
						}
					}
					Err(TrapReason::SupervisorError(error)) => Some(Err(error.into())),
				}
			}
		}
	}
}

impl<'a, T: Config, H: PrecompileHandle, M: PolkaVmInstance> Runtime<'a, T, H, M> {
	pub fn new(handle: &'a mut H, input_data: Vec<u8>, gas_limit: polkavm::Gas) -> Self {
		Self {
			handle,
			input_data: Some(input_data),
			last_gas: gas_limit,
			_phantom_data: Default::default(),
		}
	}

	/// Charge the gas meter with the specified token.
	///
	/// Returns `Err(HostError)` if there is not enough gas.
	pub(crate) fn charge_gas(&mut self, costs: RuntimeCosts) -> Result<(), SupervisorError> {
		let weight = costs.weight::<T>();
		self.handle
			.record_external_cost(Some(weight.ref_time()), Some(weight.proof_size()), None)
			.map_err(|_| SupervisorError::OutOfGas)?;

		Ok(())
	}

	pub(crate) fn charge_polkavm_gas(&mut self, memory: &mut M) -> Result<(), SupervisorError> {
		let gas = self.last_gas - memory.gas();
		if gas < 0 {
			return Err(SupervisorError::OutOfGas);
		}

		self.handle
			.record_cost(gas as u64)
			.map_err(|_| SupervisorError::OutOfGas)?;

		self.last_gas = memory.gas();
		Ok(())
	}

	/// Write the given buffer and its length to the designated locations in sandbox memory and
	/// charge gas according to the token returned by `create_token`.
	///
	/// `out_ptr` is the location in sandbox memory where `buf` should be written to.
	/// `out_len_ptr` is an in-out location in sandbox memory. It is read to determine the
	/// length of the buffer located at `out_ptr`. If that buffer is smaller than the actual
	/// `buf.len()`, only what fits into that buffer is written to `out_ptr`.
	/// The actual amount of bytes copied to `out_ptr` is written to `out_len_ptr`.
	///
	/// If `out_ptr` is set to the sentinel value of `SENTINEL` and `allow_skip` is true the
	/// operation is skipped and `Ok` is returned. This is supposed to help callers to make copying
	/// output optional. For example to skip copying back the output buffer of an `seal_call`
	/// when the caller is not interested in the result.
	///
	/// `create_token` can optionally instruct this function to charge the gas meter with the token
	/// it returns. `create_token` receives the variable amount of bytes that are about to be copied
	/// by this function.
	///
	/// In addition to the error conditions of `Memory::write` this functions returns
	/// `Err` if the size of the buffer located at `out_ptr` is too small to fit `buf`.
	pub fn write_sandbox_output(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len_ptr: u32,
		buf: &[u8],
		allow_skip: bool,
		create_token: impl FnOnce(u32) -> Option<RuntimeCosts>,
	) -> Result<(), SupervisorError> {
		if allow_skip && out_ptr == SENTINEL {
			return Ok(());
		}

		let len = memory.read_u32(out_len_ptr)?;
		let buf_len = len.min(buf.len() as u32);

		if let Some(costs) = create_token(buf_len) {
			self.charge_gas(costs)?;
		}

		memory.write(out_ptr, &buf[..buf_len as usize])?;
		memory.write(out_len_ptr, &buf_len.encode())
	}

	/// Same as `write_sandbox_output` but for static size output.
	pub fn write_fixed_sandbox_output(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		buf: &[u8],
		allow_skip: bool,
		create_token: impl FnOnce(u32) -> Option<RuntimeCosts>,
	) -> Result<(), SupervisorError> {
		if buf.is_empty() || (allow_skip && out_ptr == SENTINEL) {
			return Ok(());
		}

		let buf_len = buf.len() as u32;
		if let Some(costs) = create_token(buf_len) {
			self.charge_gas(costs)?;
		}

		memory.write(out_ptr, buf)
	}
}

// This is the API exposed to contracts.
//
// # Note
//
// Any input that leads to a out of bound error (reading or writing) or failing to decode
// data passed to the supervisor will lead to a trap. This is not documented explicitly
// for every function.
#[define_env]
pub mod env {
	/// Noop function used to benchmark the time it takes to execute an empty function.
	///
	/// Marked as stable because it needs to be called from benchmarks even when the benchmarked
	/// parachain has unstable functions disabled.
	#[cfg(feature = "runtime-benchmarks")]
	#[stable]
	fn noop(&mut self, memory: &mut M) -> Result<(), TrapReason> {
		Ok(())
	}

	/// Returns the total size of the contract call input data.
	/// See [`pallet_evm_polkavm_uapi::HostFn::call_data_size `].
	#[stable]
	fn call_data_size(&mut self, memory: &mut M) -> Result<u64, TrapReason> {
		self.charge_gas(RuntimeCosts::CallDataSize)?;
		Ok(self
			.input_data
			.as_ref()
			.map(|input| input.len().try_into().expect("usize fits into u64; qed"))
			.unwrap_or_default())
	}

	/// Stores the input passed by the caller into the supplied buffer.
	/// See [`pallet_evm_polkavm_uapi::HostFn::call_data_copy`].
	#[stable]
	fn call_data_copy(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		out_len: u32,
		offset: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::CallDataCopy(out_len))?;

		let Some(input) = self.input_data.as_ref() else {
			return Err(SupervisorError::InputForwarded.into());
		};

		let start = offset as usize;
		if start >= input.len() {
			memory.zero(out_ptr, out_len)?;
			return Ok(());
		}

		let end = start.saturating_add(out_len as usize).min(input.len());
		memory.write(out_ptr, &input[start..end])?;

		let bytes_written = (end - start) as u32;
		memory.zero(
			out_ptr.saturating_add(bytes_written),
			out_len - bytes_written,
		)?;

		Ok(())
	}

	/// Stores the U256 value at given call input `offset` into the supplied buffer.
	/// See [`pallet_evm_polkavm_uapi::HostFn::call_data_load`].
	#[stable]
	fn call_data_load(
		&mut self,
		memory: &mut M,
		out_ptr: u32,
		offset: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::CallDataLoad)?;

		let Some(input) = self.input_data.as_ref() else {
			return Err(SupervisorError::InputForwarded.into());
		};

		let mut data = [0; 32];
		let start = offset as usize;
		let data = if start >= input.len() {
			data // Any index is valid to request; OOB offsets return zero.
		} else {
			let end = start.saturating_add(32).min(input.len());
			data[..end - start].copy_from_slice(&input[start..end]);
			data.reverse();
			data // Solidity expects right-padded data
		};

		self.write_fixed_sandbox_output(memory, out_ptr, &data, false, already_charged)?;

		Ok(())
	}

	/// Cease contract execution and save a data buffer as a result of the execution.
	/// See [`pallet_evm_polkavm_uapi::HostFn::return_value`].
	#[stable]
	fn seal_return(
		&mut self,
		memory: &mut M,
		flags: u32,
		data_ptr: u32,
		data_len: u32,
	) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::CopyFromContract(data_len))?;
		Err(TrapReason::Return(ReturnData {
			flags,
			data: memory.read(data_ptr, data_len)?,
		}))
	}

	/// Stores the address of the caller into the supplied buffer.
	/// See [`pallet_evm_polkavm_uapi::HostFn::caller`].
	#[stable]
	fn caller(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Caller)?;
		let caller = self.handle.context().caller;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			caller.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Stores the address of the call stack origin into the supplied buffer.
	/// See [`pallet_evm_polkavm_uapi::HostFn::origin`].
	#[stable]
	fn origin(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Origin)?;
		let origin = self.handle.origin();
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			origin.as_bytes(),
			false,
			already_charged,
		)?)
	}

	/// Stores the address of the current contract into the supplied buffer.
	/// See [`pallet_evm_polkavm_uapi::HostFn::address`].
	#[stable]
	fn address(&mut self, memory: &mut M, out_ptr: u32) -> Result<(), TrapReason> {
		self.charge_gas(RuntimeCosts::Address)?;
		let address = self.handle.context().address;
		Ok(self.write_fixed_sandbox_output(
			memory,
			out_ptr,
			address.as_bytes(),
			false,
			already_charged,
		)?)
	}
}
