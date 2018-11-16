// Copyright 2015-2017 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::cell::Cell;
#[cfg(feature = "std")]
use std::fmt;
#[cfg(feature = "std")]
use rustc_hex::ToHex;
use impls::decode_usize;
use {Decodable, DecoderError};

#[cfg(not(feature = "std"))]
use alloc::prelude::*;

/// rlp offset
#[derive(Copy, Clone, Debug)]
struct OffsetCache {
	index: usize,
	offset: usize,
}

impl OffsetCache {
	fn new(index: usize, offset: usize) -> OffsetCache {
		OffsetCache {
			index: index,
			offset: offset,
		}
	}
}

#[derive(Debug)]
/// RLP prototype
pub enum Prototype {
	/// Empty
	Null,
	/// Value
	Data(usize),
	/// List
	List(usize),
}

/// Stores basic information about item
#[derive(Debug)]
pub struct PayloadInfo {
	/// Header length in bytes
	pub header_len: usize,
	/// Value length in bytes
	pub value_len: usize,
}

fn calculate_payload_info(header_bytes: &[u8], len_of_len: usize) -> Result<PayloadInfo, DecoderError> {
	let header_len = 1 + len_of_len;
	match header_bytes.get(1) {
		Some(&0) => return Err(DecoderError::RlpDataLenWithZeroPrefix),
		None => return Err(DecoderError::RlpIsTooShort),
		_ => (),
	}
	if header_bytes.len() < header_len {
		return Err(DecoderError::RlpIsTooShort);
	}
	let value_len = decode_usize(&header_bytes[1..header_len])?;
	if value_len <= 55 {
		return Err(DecoderError::RlpInvalidIndirection);
	}
	Ok(PayloadInfo::new(header_len, value_len))
}

impl PayloadInfo {
	fn new(header_len: usize, value_len: usize) -> PayloadInfo {
		PayloadInfo {
			header_len: header_len,
			value_len: value_len,
		}
	}

	/// Total size of the RLP.
	pub fn total(&self) -> usize { self.header_len + self.value_len }

	/// Create a new object from the given bytes RLP. The bytes
	pub fn from(header_bytes: &[u8]) -> Result<PayloadInfo, DecoderError> {
		let l = *header_bytes.first().ok_or_else(|| DecoderError::RlpIsTooShort)?;
		if l <= 0x7f {
			Ok(PayloadInfo::new(0, 1))
		} else if l <= 0xb7 {
			Ok(PayloadInfo::new(1, l as usize - 0x80))
		} else if l <= 0xbf {
			let len_of_len = l as usize - 0xb7;
			calculate_payload_info(header_bytes, len_of_len)
		} else if l <= 0xf7 {
			Ok(PayloadInfo::new(1, l as usize - 0xc0))
		} else {
			let len_of_len = l as usize - 0xf7;
			calculate_payload_info(header_bytes, len_of_len)
		}
	}
}

/// Data-oriented view onto rlp-slice.
///
/// This is an immutable structure. No operations change it.
///
/// Should be used in places where, error handling is required,
/// eg. on input
#[derive(Debug, Clone)]
pub struct Rlp<'a> {
	bytes: &'a [u8],
	offset_cache: Cell<Option<OffsetCache>>,
	count_cache: Cell<Option<usize>>,
}

#[cfg(feature = "std")]
impl<'a> fmt::Display for Rlp<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		match self.prototype() {
			Ok(Prototype::Null) => write!(f, "null"),
			Ok(Prototype::Data(_)) => write!(f, "\"0x{}\"", self.data().unwrap().to_hex::<String>()),
			Ok(Prototype::List(len)) => {
				write!(f, "[")?;
				for i in 0..len-1 {
					write!(f, "{}, ", self.at(i).unwrap())?;
				}
				write!(f, "{}", self.at(len - 1).unwrap())?;
				write!(f, "]")
			},
			Err(err) => write!(f, "{:?}", err)
		}
	}
}

impl<'a> Rlp<'a> {
	pub fn new(bytes: &'a [u8]) -> Rlp<'a> {
		Rlp {
			bytes: bytes,
			offset_cache: Cell::new(None),
			count_cache: Cell::new(None)
		}
	}

	pub fn as_raw<'view>(&'view self) -> &'a [u8] where 'a: 'view {
		self.bytes
	}

	pub fn prototype(&self) -> Result<Prototype, DecoderError> {
		// optimize? && return appropriate errors
		if self.is_data() {
			Ok(Prototype::Data(self.size()))
		} else if self.is_list() {
			self.item_count().map(Prototype::List)
		} else {
			Ok(Prototype::Null)
		}
	}

	pub fn payload_info(&self) -> Result<PayloadInfo, DecoderError> {
		BasicDecoder::payload_info(self.bytes)
	}

	pub fn data<'view>(&'view self) -> Result<&'a [u8], DecoderError> where 'a: 'view {
		let pi = BasicDecoder::payload_info(self.bytes)?;
		Ok(&self.bytes[pi.header_len..(pi.header_len + pi.value_len)])
	}

	pub fn item_count(&self) -> Result<usize, DecoderError> {
		match self.is_list() {
			true => match self.count_cache.get() {
				Some(c) => Ok(c),
				None => {
					let c = self.iter().count();
					self.count_cache.set(Some(c));
					Ok(c)
				}
			},
			false => Err(DecoderError::RlpExpectedToBeList),
		}
	}

	pub fn size(&self) -> usize {
		match self.is_data() {
			// TODO: No panic on malformed data, but ideally would Err on no PayloadInfo.
			true => BasicDecoder::payload_info(self.bytes).map(|b| b.value_len).unwrap_or(0),
			false => 0
		}
	}

	pub fn at<'view>(&'view self, index: usize) -> Result<Rlp<'a>, DecoderError> where 'a: 'view {
		if !self.is_list() {
			return Err(DecoderError::RlpExpectedToBeList);
		}

		// move to cached position if its index is less or equal to
		// current search index, otherwise move to beginning of list
		let cache = self.offset_cache.get();
		let (bytes, indexes_to_skip, bytes_consumed) = match cache {
			Some(ref cache) if cache.index <= index => (
				Rlp::consume(self.bytes, cache.offset)?, index - cache.index, cache.offset
			),
			_ => {
				let (bytes, consumed) = self.consume_list_payload()?;
				(bytes, index, consumed)
			}
		};

		// skip up to x items
		let (bytes, consumed) = Rlp::consume_items(bytes, indexes_to_skip)?;

		// update the cache
		self.offset_cache.set(Some(OffsetCache::new(index, bytes_consumed + consumed)));

		// construct new rlp
		let found = BasicDecoder::payload_info(bytes)?;
		Ok(Rlp::new(&bytes[0..found.header_len + found.value_len]))
	}

	pub fn is_null(&self) -> bool {
		self.bytes.len() == 0
	}

	pub fn is_empty(&self) -> bool {
		!self.is_null() && (self.bytes[0] == 0xc0 || self.bytes[0] == 0x80)
	}

	pub fn is_list(&self) -> bool {
		!self.is_null() && self.bytes[0] >= 0xc0
	}

	pub fn is_data(&self) -> bool {
		!self.is_null() && self.bytes[0] < 0xc0
	}

	pub fn is_int(&self) -> bool {
		if self.is_null() {
			return false;
		}

		match self.bytes[0] {
			0...0x80 => true,
			0x81...0xb7 => self.bytes[1] != 0,
			b @ 0xb8...0xbf => {
				let payload_idx = 1 + b as usize - 0xb7;
				payload_idx < self.bytes.len() && self.bytes[payload_idx] != 0
			},
			_ => false
		}
	}

	pub fn iter<'view>(&'view self) -> RlpIterator<'a, 'view> where 'a: 'view {
		self.into_iter()
	}

	pub fn as_val<T>(&self) -> Result<T, DecoderError> where T: Decodable {
		T::decode(self)
	}

	pub fn as_list<T>(&self) -> Result<Vec<T>, DecoderError> where T: Decodable {
		self.iter().map(|rlp| rlp.as_val()).collect()
	}

	pub fn val_at<T>(&self, index: usize) -> Result<T, DecoderError> where T: Decodable {
		self.at(index)?.as_val()
	}

	pub fn list_at<T>(&self, index: usize) -> Result<Vec<T>, DecoderError> where T: Decodable {
		self.at(index)?.as_list()
	}

	pub fn decoder(&self) -> BasicDecoder {
		BasicDecoder::new(self.bytes)
	}

	/// consumes first found prefix
	fn consume_list_payload(&self) -> Result<(&'a [u8], usize), DecoderError> {
		let item = BasicDecoder::payload_info(self.bytes)?;
		if self.bytes.len() < (item.header_len + item.value_len) {
			return Err(DecoderError::RlpIsTooShort);
		}
		Ok((&self.bytes[item.header_len..item.header_len + item.value_len], item.header_len))
	}

	/// consumes fixed number of items
	fn consume_items(bytes: &'a [u8], items: usize) -> Result<(&'a [u8], usize), DecoderError> {
		let mut result = bytes;
		let mut consumed = 0;
		for _ in 0..items {
			let i = BasicDecoder::payload_info(result)?;
			let to_consume = i.header_len + i.value_len;
			result = Rlp::consume(result, to_consume)?;
			consumed += to_consume;
		}
		Ok((result, consumed))
	}

	/// consumes slice prefix of length `len`
	fn consume(bytes: &'a [u8], len: usize) -> Result<&'a [u8], DecoderError> {
		match bytes.len() >= len {
			true => Ok(&bytes[len..]),
			false => Err(DecoderError::RlpIsTooShort)
		}
	}
}

/// Iterator over rlp-slice list elements.
pub struct RlpIterator<'a, 'view> where 'a: 'view {
	rlp: &'view Rlp<'a>,
	index: usize,
}

impl<'a, 'view> IntoIterator for &'view Rlp<'a> where 'a: 'view {
	type Item = Rlp<'a>;
	type IntoIter = RlpIterator<'a, 'view>;

	fn into_iter(self) -> Self::IntoIter {
		RlpIterator {
			rlp: self,
			index: 0,
		}
	}
}

impl<'a, 'view> Iterator for RlpIterator<'a, 'view> {
	type Item = Rlp<'a>;

	fn next(&mut self) -> Option<Rlp<'a>> {
		let index = self.index;
		let result = self.rlp.at(index).ok();
		self.index += 1;
		result
	}
}

pub struct BasicDecoder<'a> {
	rlp: &'a [u8],
}

impl<'a> BasicDecoder<'a> {
	pub fn new(rlp: &'a [u8]) -> BasicDecoder<'a> {
		BasicDecoder {
			rlp,
		}
	}

	/// Return first item info.
	fn payload_info(bytes: &[u8]) -> Result<PayloadInfo, DecoderError> {
		let item = PayloadInfo::from(bytes)?;
		match item.header_len.checked_add(item.value_len) {
			Some(x) if x <= bytes.len() => Ok(item),
			_ => Err(DecoderError::RlpIsTooShort),
		}
	}

	pub fn decode_value<T, F>(&self, f: F) -> Result<T, DecoderError>
		where F: Fn(&[u8]) -> Result<T, DecoderError> {

		let bytes = self.rlp;

		let l = *bytes.first().ok_or_else(|| DecoderError::RlpIsTooShort)?;

		if l <= 0x7f {
			Ok(f(&[l])?)
		} else if l <= 0xb7 {
			let last_index_of = 1 + l as usize - 0x80;
			if bytes.len() < last_index_of {
				return Err(DecoderError::RlpInconsistentLengthAndData);
			}
			let d = &bytes[1..last_index_of];
			if l == 0x81 && d[0] < 0x80 {
				return Err(DecoderError::RlpInvalidIndirection);
			}
			Ok(f(d)?)
		} else if l <= 0xbf {
			let len_of_len = l as usize - 0xb7;
			let begin_of_value = 1 as usize + len_of_len;
			if bytes.len() < begin_of_value {
				return Err(DecoderError::RlpInconsistentLengthAndData);
			}
			let len = decode_usize(&bytes[1..begin_of_value])?;

			let last_index_of_value = begin_of_value.checked_add(len)
				.ok_or(DecoderError::RlpInvalidLength)?;
			if bytes.len() < last_index_of_value {
				return Err(DecoderError::RlpInconsistentLengthAndData);
			}
			Ok(f(&bytes[begin_of_value..last_index_of_value])?)
		} else {
			Err(DecoderError::RlpExpectedToBeData)
		}
	}
}

#[cfg(test)]
mod tests {
	use {Rlp, DecoderError};

	#[test]
	fn test_rlp_display() {
		let data = hex!("f84d0589010efbef67941f79b2a056e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421a0c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
		let rlp = Rlp::new(&data);
		assert_eq!(format!("{}", rlp), "[\"0x05\", \"0x010efbef67941f79b2\", \"0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421\", \"0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470\"]");
	}

	#[test]
	fn length_overflow() {
		let bs = [0xbf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xe5];
		let rlp = Rlp::new(&bs);
		let res: Result<u8, DecoderError> = rlp.as_val();
		assert_eq!(Err(DecoderError::RlpInvalidLength), res);
	}
}
