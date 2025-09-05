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
	fn call_with_code_per_byte(c: u32) -> Weight;
	fn noop_host_fn(r: u32) -> Weight;
	fn seal_caller() -> Weight;
	fn seal_origin() -> Weight;
	fn seal_address() -> Weight;
	fn seal_call_data_size() -> Weight;
	fn seal_call_data_load() -> Weight;
	fn seal_call_data_copy(n: u32) -> Weight;
	fn seal_return(n: u32) -> Weight;
	fn create_polkavm(l: u32) -> Weight;
}
