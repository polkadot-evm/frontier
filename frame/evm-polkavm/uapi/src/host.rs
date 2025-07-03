// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use crate::ReturnFlags;

#[cfg(target_arch = "riscv64")]
mod riscv64;

/// Implements [`HostFn`] when compiled on supported architectures (RISC-V).
pub enum HostFnImpl {}

/// Defines all the host apis available to contracts.
pub trait HostFn: private::Sealed {
	/// Stores the address of the current contract into the supplied buffer.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the address.
	fn address(output: &mut [u8; 20]);

	/// Stores the U256 value at given `offset` from the input passed by the caller
	/// into the supplied buffer.
	///
	/// # Note
	/// - If `offset` is out of bounds, a value of zero will be returned.
	/// - If `offset` is in bounds but there is not enough call data, the available data
	///   is right-padded in order to fill a whole U256 value.
	/// - The data written to `output` is a little endian U256 integer value.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the fixed output data buffer to write the value.
	/// - `offset`: The offset (index) into the call data.
	fn call_data_load(output: &mut [u8; 32], offset: u32);

	/// Returns the call data size.
	fn call_data_size() -> u64;

	/// Stores the input data passed by the caller into the supplied `output` buffer,
	/// starting from the given input data `offset`.
	///
	/// The `output` buffer is guaranteed to always be fully populated:
	/// - If the call data (starting from the given `offset`) is larger than the `output` buffer,
	///   only what fits into the `output` buffer is written.
	/// - If the `output` buffer size exceeds the call data size (starting from `offset`), remaining
	///   bytes in the `output` buffer are zeroed out.
	/// - If the provided call data `offset` is out-of-bounds, the whole `output` buffer is zeroed
	///   out.
	///
	/// # Note
	///
	/// This function traps if:
	/// - the input was previously forwarded by a [`call()`][`Self::call()`].
	/// - the `output` buffer is located in an PolkaVM invalid memory range.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the call data.
	/// - `offset`: The offset index into the call data from where to start copying.
	fn call_data_copy(output: &mut [u8], offset: u32);

	/// Stores the address of the caller into the supplied buffer.
	///
	/// If this is a top-level call (i.e. initiated by an extrinsic) the origin address of the
	/// extrinsic will be returned. Otherwise, if this call is initiated by another contract then
	/// the address of the contract will be returned.
	///
	/// If there is no address associated with the caller (e.g. because the caller is root) then
	/// it traps with `BadOrigin`.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the caller address.
	fn caller(output: &mut [u8; 20]);

	/// Stores the origin address (initator of the call stack) into the supplied buffer.
	///
	/// If there is no address associated with the origin (e.g. because the origin is root) then
	/// it traps with `BadOrigin`. This can only happen through on-chain governance actions or
	/// customized runtimes.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the origin's address.
	fn origin(output: &mut [u8; 20]);

	/// Deposit a contract event with the data buffer and optional list of topics. There is a limit
	/// on the maximum number of topics specified by `event_topics`.
	///
	/// There should not be any duplicates in `topics`.
	///
	/// # Parameters
	///
	/// - `topics`: The topics list. It can't contain duplicates.
	fn deposit_event(topics: &[[u8; 32]], data: &[u8]);

	/// Cease contract execution and save a data buffer as a result of the execution.
	///
	/// This function never returns as it stops execution of the caller.
	/// This is the only way to return a data buffer to the caller. Returning from
	/// execution without calling this function is equivalent to calling:
	/// ```nocompile
	/// return_value(ReturnFlags::empty(), &[])
	/// ```
	///
	/// Using an unnamed non empty `ReturnFlags` triggers a trap.
	///
	/// # Parameters
	///
	/// - `flags`: Flag used to signal special return conditions to the supervisor. See
	///   [`ReturnFlags`] for a documentation of the supported flags.
	/// - `return_value`: The return value buffer.
	fn return_value(flags: ReturnFlags, return_value: &[u8]) -> !;
}

mod private {
	pub trait Sealed {}
	impl Sealed for super::HostFnImpl {}
}
