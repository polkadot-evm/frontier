// This file is part of Tokfin.

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

// Few mock structs to check the macro.
struct PrecompileAt<T, U, V = ()>(PhantomData<(T, U, V)>);
struct AddressU64<const N: u64>;
struct FooPrecompile<R>(PhantomData<R>);
struct BarPrecompile<R, S>(PhantomData<(R, S)>);
struct MockCheck;

#[precompile_utils_macro::precompile_name_from_address]
type Precompiles = (
	PrecompileAt<AddressU64<1>, FooPrecompile<R>>,
	PrecompileAt<AddressU64<2>, BarPrecompile<R, S>, (MockCheck, MockCheck)>,
);
