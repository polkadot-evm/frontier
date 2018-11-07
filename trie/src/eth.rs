// Copyright 2015-2018 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Ethereum trie codec.

use std::marker::PhantomData;
use substrate_primitives::H256;
use keccak_hasher::KeccakHasher;
use codec::{Encode, Decode, Compact};
use hash_db::Hasher;
use trie_db::{self, DBValue, NibbleSlice, node::Node, ChildReference};
use error::Error;
use super::{
	EMPTY_TRIE, LEAF_NODE_OFFSET, LEAF_NODE_BIG, EXTENSION_NODE_OFFSET,
	EXTENSION_NODE_BIG, take, partial_to_key, node_header::NodeHeader, branch_node
};

#[derive(Default, Clone)]
pub struct EthereumCodec;

impl trie_db::NodeCodec<KeccakHasher> for EthereumCodec {
	type Error = Error;

	fn hashed_null_node() -> H256 {
		H256(
			[0x56, 0xe8, 0x1f, 0x17,
			 0x1b, 0xcc, 0x55, 0xa6,
			 0xff, 0x83, 0x45, 0xe6,
			 0x92, 0xc0, 0xf8, 0x6e,
			 0x5b, 0x48, 0xe0, 0x1b,
			 0x99, 0x6c, 0xad, 0xc0,
			 0x01, 0x62, 0x2f, 0xb5,
			 0xe3, 0x63, 0xb4, 0x21]
		)
	}

	fn decode(data: &[u8]) -> ::std::result::Result<Node, Self::Error> {
		unimplemented!()
	}

	fn try_decode_hash(data: &[u8]) -> Option<H256> {
		unimplemented!()
	}

	fn is_empty_node(data: &[u8]) -> bool {
		unimplemented!()
	}

	fn empty_node() -> Vec<u8> {
		unimplemented!()
	}

	fn leaf_node(partial: &[u8], value: &[u8]) -> Vec<u8> {
		unimplemented!()
	}

	fn ext_node(partial: &[u8], child: ChildReference<H256>) -> Vec<u8> {
		unimplemented!()
	}

	fn branch_node<I>(children: I, maybe_value: Option<DBValue>) -> Vec<u8>
		where I: IntoIterator<Item=Option<ChildReference<H256>>> + Iterator<Item=Option<ChildReference<H256>>>
	{
		unimplemented!()
	}
}
