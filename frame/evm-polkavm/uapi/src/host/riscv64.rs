// This file is part of Tokfin.

// Copyright (C) Tokfin developers.
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

#![allow(unused_variables)]

use crate::{
	host::{CallFlags, HostFn, HostFnImpl, Result, StorageFlags},
	pack_hi_lo, ReturnFlags,
};
use pallet_revive_proc_macro::unstable_hostfn;

mod sys {
	use crate::ReturnCode;

	#[polkavm_derive::polkavm_define_abi]
	mod abi {}

	impl abi::FromHost for ReturnCode {
		type Regs = (u64,);

		fn from_host((a0,): Self::Regs) -> Self {
			ReturnCode(a0 as _)
		}
	}

	#[polkavm_derive::polkavm_import(abi = self::abi)]
	extern "C" {
		pub fn call_data_size() -> u64;
		pub fn call_data_copy(out_ptr: *mut u8, out_len: u32, offset: u32);
		pub fn call_data_load(out_ptr: *mut u8, offset: u32);
		pub fn seal_return(flags: u32, data_ptr: *const u8, data_len: u32);
		pub fn caller(out_ptr: *mut u8);
		pub fn origin(out_ptr: *mut u8);
		pub fn address(out_ptr: *mut u8);
		pub fn deposit_event(
			topics_ptr: *const [u8; 32],
			num_topic: u32,
			data_ptr: *const u8,
			data_len: u32,
		);
	}
}

#[inline(always)]
fn extract_from_slice(output: &mut &mut [u8], new_len: usize) {
	debug_assert!(new_len <= output.len());
	let tmp = core::mem::take(output);
	*output = &mut tmp[..new_len];
}

#[inline(always)]
fn ptr_len_or_sentinel(data: &mut Option<&mut &mut [u8]>) -> (*mut u8, u32) {
	match data {
		Some(ref mut data) => (data.as_mut_ptr(), data.len() as _),
		None => (crate::SENTINEL as _, 0),
	}
}

#[inline(always)]
fn ptr_or_sentinel(data: &Option<&[u8; 32]>) -> *const u8 {
	match data {
		Some(ref data) => data.as_ptr(),
		None => crate::SENTINEL as _,
	}
}

impl HostFn for HostFnImpl {
	fn deposit_event(topics: &[[u8; 32]], data: &[u8]) {
		unsafe {
			sys::deposit_event(
				topics.as_ptr(),
				topics.len() as u32,
				data.as_ptr(),
				data.len() as u32,
			)
		}
	}

	fn call_data_load(out_ptr: &mut [u8; 32], offset: u32) {
		unsafe { sys::call_data_load(out_ptr.as_mut_ptr(), offset) };
	}

	fn call_data_size() -> u64 {
		unsafe { sys::call_data_size() }
	}

	fn call_data_copy(output: &mut [u8], offset: u32) {
		let len = output.len() as u32;
		unsafe { sys::call_data_copy(output.as_mut_ptr(), len, offset) };
	}

	fn return_value(flags: ReturnFlags, return_value: &[u8]) -> ! {
		unsafe {
			sys::seal_return(
				flags.bits(),
				return_value.as_ptr(),
				return_value.len() as u32,
			)
		}
		panic!("seal_return does not return");
	}

	fn address(output: &mut [u8; 20]) {
		unsafe { sys::address(output.as_mut_ptr()) }
	}

	fn caller(output: &mut [u8; 20]) {
		unsafe { sys::caller(output.as_mut_ptr()) }
	}

	fn origin(output: &mut [u8; 20]) {
		unsafe { sys::origin(output.as_mut_ptr()) }
	}
}
