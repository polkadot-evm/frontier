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

use substrate_primitives::H256;
use keccak_hasher::KeccakHasher;
use hash_db::{Hasher, HashDB, PlainDB, HashDBRef, AsHashDB};
use std::marker::PhantomData;
use trie_db::{self, DBValue, NibbleSlice, node::Node, ChildReference, Query};
use rlp::{DecoderError, RlpStream, Rlp, Prototype};

pub struct BridgedQuery<H: Hasher, I, Q: Query<H, Item=I>> {
	query: Q,
	_marker: PhantomData<(H, I)>,
}

impl<H: Hasher, I, Q: Query<H, Item=I>> BridgedQuery<H, I, Q> {
	pub fn new(query: Q) -> Self {
		Self {
			query,
			_marker: PhantomData,
		}
	}
}

impl<H: Hasher, I, Q: Query<H, Item=I>> Query<KeccakHasher> for BridgedQuery<H, I, Q> {
	type Item = Q::Item;

	fn decode(self, data: &[u8]) -> Self::Item {
		self.query.decode(data)
	}

	fn record(&mut self, hash: &H256, data: &[u8], depth: u32) {
		let ohash = H::hash(hash.as_ref());
		self.query.record(&ohash, data, depth)
	}
}

pub struct BridgedHashDB<'a, HS: Hasher + 'a> {
	db: &'a HashDBRef<HS, trie_db::DBValue>,
	_marker: PhantomData<HS>,
}

impl<'a, HS: Hasher> BridgedHashDB<'a, HS> {
	pub fn new(db: &'a HashDBRef<HS, trie_db::DBValue>) -> Self {
		BridgedHashDB {
			db,
			_marker: PhantomData,
		}
	}
}

impl<'a, 'b, HS: Hasher> HashDBRef<KeccakHasher, trie_db::DBValue> for BridgedHashDB<'a, HS> {
	fn get(&self, key: &H256) -> Option<trie_db::DBValue> {
		let okey = HS::hash(key.as_ref());
		self.db.get(&okey)
	}

	fn contains(&self, key: &H256) -> bool {
		let okey = HS::hash(key.as_ref());
		self.db.contains(&okey)
	}
}

pub struct BridgedHashDBMut<'a, HS: Hasher + 'a> {
	db: &'a mut PlainDB<HS::Out, trie_db::DBValue>,
	hashed_null_node: H256,
	null_node_data: trie_db::DBValue,
	_marker: PhantomData<HS>,
}

impl<'a, HS: Hasher> BridgedHashDBMut<'a, HS> {
	pub fn new(db: &'a mut PlainDB<HS::Out, trie_db::DBValue>) -> Self {
		BridgedHashDBMut {
			db,
			hashed_null_node: KeccakHasher::hash(&[128u8][..]),
			null_node_data: trie_db::DBValue::from(&[128u8][..]),
			_marker: PhantomData,
		}
	}
}

impl<'a, HS: Hasher> AsHashDB<KeccakHasher, trie_db::DBValue> for BridgedHashDBMut<'a, HS> {
	fn as_hash_db<'b>(&'b self) -> &'b (hash_db::HashDB<KeccakHasher, trie_db::DBValue> + 'b) { self }
	fn as_hash_db_mut<'b>(&'b mut self) -> &'b mut (hash_db::HashDB<KeccakHasher, trie_db::DBValue> + 'b) { self }
}

impl<'a, HS: Hasher> HashDB<KeccakHasher, trie_db::DBValue> for BridgedHashDBMut<'a, HS> {
	fn get(&self, key: &H256) -> Option<trie_db::DBValue> {
		if key == &self.hashed_null_node {
			return Some(self.null_node_data.clone());
		}

		let okey = HS::hash(key.as_ref());
		self.db.get(&okey)
	}

	fn contains(&self, key: &H256) -> bool {
		if key == &self.hashed_null_node {
			return true;
		}

		let okey = HS::hash(key.as_ref());
		self.db.contains(&okey)
	}

	fn insert(&mut self, value: &[u8]) -> H256 {
		if trie_db::DBValue::from(value) == self.null_node_data {
			return self.hashed_null_node.clone();
		}

		let key = KeccakHasher::hash(value);
		self.emplace(key, value.into());
		key
	}

	fn emplace(&mut self, key: H256, value: trie_db::DBValue) {
		if value == self.null_node_data {
			return;
		}

		let okey = HS::hash(key.as_ref());
		self.db.emplace(okey, value)
	}

	fn remove(&mut self, key: &H256) {
		if key == &self.hashed_null_node {
			return;
		}

		let okey = HS::hash(key.as_ref());
		self.db.remove(&okey)
	}
}

#[derive(Default, Clone)]
pub struct EthereumCodec;

impl trie_db::NodeCodec<KeccakHasher> for EthereumCodec {
	type Error = DecoderError;

	fn hashed_null_node() -> H256 {
		H256::from(
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
		let r = Rlp::new(data);
		match r.prototype()? {
			// either leaf or extension - decode first item with NibbleSlice::???
			// and use is_leaf return to figure out which.
			// if leaf, second item is a value (is_data())
			// if extension, second item is a node (either SHA3 to be looked up and
			// fed back into this function or inline RLP which can be fed back into this function).
			Prototype::List(2) => match NibbleSlice::from_encoded(r.at(0)?.data()?) {
				(slice, true) => Ok(Node::Leaf(slice, r.at(1)?.data()?)),
				(slice, false) => Ok(Node::Extension(slice, r.at(1)?.as_raw())),
			},
			// branch - first 16 are nodes, 17th is a value (or empty).
			Prototype::List(17) => {
				let mut nodes = [Some(&[] as &[u8]); 16];
				for i in 0..16 {
					nodes[i] = Some(r.at(i)?.as_raw());
				}
				Ok(Node::Branch(nodes, if r.at(16)?.is_empty() { None } else { Some(r.at(16)?.data()?) }))
			},
			// an empty branch index.
			Prototype::Data(0) => Ok(Node::Empty),
			// something went wrong.
			_ => Err(DecoderError::Custom("Rlp is not valid."))
		}
	}

	fn try_decode_hash(data: &[u8]) -> Option<H256> {
		let r = Rlp::new(data);
		if r.is_data() && r.size() == KeccakHasher::LENGTH {
			Some(r.as_val().expect("Hash is the correct size; qed"))
		} else {
			None
		}
	}

	fn is_empty_node(data: &[u8]) -> bool {
		Rlp::new(data).is_empty()
	}

	fn empty_node() -> Vec<u8> {
		let mut stream = RlpStream::new();
		stream.append_empty_data();
		stream.drain()
	}

	fn leaf_node(partial: &[u8], value: &[u8]) -> Vec<u8> {
		let mut stream = RlpStream::new_list(2);
		stream.append(&partial);
		stream.append(&value);
		stream.drain()
	}

	fn ext_node(partial: &[u8], child_ref: ChildReference<H256>) -> Vec<u8> {
		let mut stream = RlpStream::new_list(2);
		stream.append(&partial);
		match child_ref {
			ChildReference::Hash(h) => stream.append(&h),
			ChildReference::Inline(inline_data, len) => {
				let bytes = &AsRef::<[u8]>::as_ref(&inline_data)[..len];
				stream.append_raw(bytes, 1)
			},
		};
		stream.drain()
	}

	fn branch_node<I>(children: I, maybe_value: Option<DBValue>) -> Vec<u8>
		where I: IntoIterator<Item=Option<ChildReference<H256>>> + Iterator<Item=Option<ChildReference<H256>>>
	{
		let mut stream = RlpStream::new_list(17);
		for child_ref in children {
			match child_ref {
				Some(c) => match c {
					ChildReference::Hash(h) => stream.append(&h),
					ChildReference::Inline(inline_data, len) => {
						let bytes = &AsRef::<[u8]>::as_ref(&inline_data)[..len];
						stream.append_raw(bytes, 1)
					},
				},
				None => stream.append_empty_data()
			};
		}
		if let Some(value) = maybe_value {
			stream.append(&&*value);
		} else {
			stream.append_empty_data();
		}
		stream.drain()
	}
}
