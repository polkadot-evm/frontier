// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
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

use fp_evm::Opcode;

// Temporary static opcode ref_time values (25_000 weight per gas)
const G_JUMPDEST: u64 = 25_000;
const G_BASE: u64 = 50_000;
const G_VERYLOW: u64 = 75_000;
const G_LOW: u64 = 125_000;
const G_MID: u64 = 200_000;
const G_HIGH: u64 = 250_000;

pub fn static_opcode_ref_time_cost(opcode: Opcode) -> Option<u64> {
	static TABLE: [Option<u64>; 256] = {
		let mut table = [None; 256];

		table[Opcode::CALLDATASIZE.as_usize()] = Some(G_BASE);
		table[Opcode::CODESIZE.as_usize()] = Some(G_BASE);
		table[Opcode::POP.as_usize()] = Some(G_BASE);
		table[Opcode::PC.as_usize()] = Some(G_BASE);
		table[Opcode::MSIZE.as_usize()] = Some(G_BASE);

		table[Opcode::ADDRESS.as_usize()] = Some(G_BASE);
		table[Opcode::ORIGIN.as_usize()] = Some(G_BASE);
		table[Opcode::CALLER.as_usize()] = Some(G_BASE);
		table[Opcode::CALLVALUE.as_usize()] = Some(G_BASE);
		table[Opcode::COINBASE.as_usize()] = Some(G_BASE);
		table[Opcode::TIMESTAMP.as_usize()] = Some(G_BASE);
		table[Opcode::NUMBER.as_usize()] = Some(G_BASE);
		table[Opcode::DIFFICULTY.as_usize()] = Some(G_BASE);
		table[Opcode::GASLIMIT.as_usize()] = Some(G_BASE);
		table[Opcode::GASPRICE.as_usize()] = Some(G_BASE);
		table[Opcode::GAS.as_usize()] = Some(G_BASE);

		table[Opcode::ADD.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SUB.as_usize()] = Some(G_VERYLOW);
		table[Opcode::NOT.as_usize()] = Some(G_VERYLOW);
		table[Opcode::LT.as_usize()] = Some(G_VERYLOW);
		table[Opcode::GT.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SLT.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SGT.as_usize()] = Some(G_VERYLOW);
		table[Opcode::EQ.as_usize()] = Some(G_VERYLOW);
		table[Opcode::ISZERO.as_usize()] = Some(G_VERYLOW);
		table[Opcode::AND.as_usize()] = Some(G_VERYLOW);
		table[Opcode::OR.as_usize()] = Some(G_VERYLOW);
		table[Opcode::XOR.as_usize()] = Some(G_VERYLOW);
		table[Opcode::BYTE.as_usize()] = Some(G_VERYLOW);
		table[Opcode::CALLDATALOAD.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH1.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH2.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH3.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH4.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH5.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH6.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH7.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH8.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH9.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH10.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH11.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH12.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH13.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH14.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH15.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH16.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH17.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH18.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH19.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH20.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH21.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH22.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH23.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH24.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH25.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH26.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH27.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH28.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH29.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH30.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH31.as_usize()] = Some(G_VERYLOW);
		table[Opcode::PUSH32.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP1.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP2.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP3.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP4.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP5.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP6.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP7.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP8.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP9.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP10.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP11.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP12.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP13.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP14.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP15.as_usize()] = Some(G_VERYLOW);
		table[Opcode::DUP16.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP1.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP2.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP3.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP4.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP5.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP6.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP7.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP8.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP9.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP10.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP11.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP12.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP13.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP14.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP15.as_usize()] = Some(G_VERYLOW);
		table[Opcode::SWAP16.as_usize()] = Some(G_VERYLOW);

		table[Opcode::MUL.as_usize()] = Some(G_LOW);
		table[Opcode::DIV.as_usize()] = Some(G_LOW);
		table[Opcode::SDIV.as_usize()] = Some(G_LOW);
		table[Opcode::MOD.as_usize()] = Some(G_LOW);
		table[Opcode::SMOD.as_usize()] = Some(G_LOW);
		table[Opcode::SIGNEXTEND.as_usize()] = Some(G_LOW);

		table[Opcode::ADDMOD.as_usize()] = Some(G_MID);
		table[Opcode::MULMOD.as_usize()] = Some(G_MID);
		table[Opcode::JUMP.as_usize()] = Some(G_MID);

		table[Opcode::JUMPI.as_usize()] = Some(G_HIGH);
		table[Opcode::JUMPDEST.as_usize()] = Some(G_JUMPDEST);

		table
	};

	TABLE[opcode.as_usize()]
}