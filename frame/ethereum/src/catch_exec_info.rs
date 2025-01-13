// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use fp_evm::CallOrCreateInfo;

environmental::environmental!(GLOBAL: Option<CallOrCreateInfo>);

// Allow to catch informations of an ethereum execution inside the provided closure.
pub fn catch_exec_info<R, F: FnOnce() -> R>(
	execution_info: &mut Option<CallOrCreateInfo>,
	f: F,
) -> R {
	GLOBAL::using(execution_info, f)
}

pub(super) fn fill_exec_info(execution_info: &CallOrCreateInfo) {
	GLOBAL::with(|exec_info| exec_info.replace(execution_info.clone()));
}
