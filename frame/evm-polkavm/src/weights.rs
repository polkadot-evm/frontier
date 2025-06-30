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

use sp_runtime::Weight;

pub trait WeightInfo {
	fn on_process_deletion_queue_batch() -> Weight;
	fn on_initialize_per_trie_key(k: u32) -> Weight;
	fn call_with_code_per_byte(c: u32) -> Weight;
	fn basic_block_compilation(b: u32) -> Weight;
	fn instantiate_with_code(c: u32, i: u32) -> Weight;
	fn instantiate(i: u32) -> Weight;
	fn call() -> Weight;
	fn upload_code(c: u32) -> Weight;
	fn remove_code() -> Weight;
	fn set_code() -> Weight;
	fn map_account() -> Weight;
	fn unmap_account() -> Weight;
	fn dispatch_as_fallback_account() -> Weight;
	fn noop_host_fn(r: u32) -> Weight;
	fn seal_caller() -> Weight;
	fn seal_origin() -> Weight;
	fn seal_to_account_id() -> Weight;
	fn seal_code_hash() -> Weight;
	fn seal_own_code_hash() -> Weight;
	fn seal_code_size() -> Weight;
	fn seal_caller_is_origin() -> Weight;
	fn seal_caller_is_root() -> Weight;
	fn seal_address() -> Weight;
	fn seal_weight_left() -> Weight;
	fn seal_ref_time_left() -> Weight;
	fn seal_balance() -> Weight;
	fn seal_balance_of() -> Weight;
	fn seal_get_immutable_data(n: u32) -> Weight;
	fn seal_set_immutable_data(n: u32) -> Weight;
	fn seal_value_transferred() -> Weight;
	fn seal_minimum_balance() -> Weight;
	fn seal_return_data_size() -> Weight;
	fn seal_call_data_size() -> Weight;
	fn seal_gas_limit() -> Weight;
	fn seal_gas_price() -> Weight;
	fn seal_base_fee() -> Weight;
	fn seal_block_number() -> Weight;
	fn seal_block_author() -> Weight;
	fn seal_block_hash() -> Weight;
	fn seal_now() -> Weight;
	fn seal_weight_to_fee() -> Weight;
	fn seal_copy_to_contract(n: u32) -> Weight;
	fn seal_call_data_load() -> Weight;
	fn seal_call_data_copy(n: u32) -> Weight;
	fn seal_return(n: u32) -> Weight;
	fn seal_terminate() -> Weight;
	fn seal_deposit_event(t: u32, n: u32) -> Weight;
	fn get_storage_empty() -> Weight;
	fn get_storage_full() -> Weight;
	fn set_storage_empty() -> Weight;
	fn set_storage_full() -> Weight;
	fn seal_set_storage(n: u32, o: u32) -> Weight;
	fn seal_clear_storage(n: u32) -> Weight;
	fn seal_get_storage(n: u32) -> Weight;
	fn seal_contains_storage(n: u32) -> Weight;
	fn seal_take_storage(n: u32) -> Weight;
	fn set_transient_storage_empty() -> Weight;
	fn set_transient_storage_full() -> Weight;
	fn get_transient_storage_empty() -> Weight;
	fn get_transient_storage_full() -> Weight;
	fn rollback_transient_storage() -> Weight;
	fn seal_set_transient_storage(n: u32, o: u32) -> Weight;
	fn seal_clear_transient_storage(n: u32) -> Weight;
	fn seal_get_transient_storage(n: u32) -> Weight;
	fn seal_contains_transient_storage(n: u32) -> Weight;
	fn seal_take_transient_storage(n: u32) -> Weight;
	fn seal_call(t: u32, i: u32) -> Weight;
	fn seal_call_precompile(d: u32, i: u32) -> Weight;
	fn seal_delegate_call() -> Weight;
	fn seal_instantiate(i: u32) -> Weight;
	fn sha2_256(n: u32) -> Weight;
	fn identity(n: u32) -> Weight;
	fn ripemd_160(n: u32) -> Weight;
	fn seal_hash_keccak_256(n: u32) -> Weight;
	fn seal_hash_blake2_256(n: u32) -> Weight;
	fn seal_hash_blake2_128(n: u32) -> Weight;
	fn seal_sr25519_verify(n: u32) -> Weight;
	fn ecdsa_recover() -> Weight;
	fn bn128_add() -> Weight;
	fn bn128_mul() -> Weight;
	fn bn128_pairing(n: u32) -> Weight;
	fn blake2f(n: u32) -> Weight;
	fn seal_ecdsa_to_eth_address() -> Weight;
	fn seal_set_code_hash() -> Weight;
	fn instr(r: u32) -> Weight;
	fn instr_empty_loop(r: u32) -> Weight;
}
