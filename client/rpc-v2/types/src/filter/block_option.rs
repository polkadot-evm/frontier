// This file is part of Tokfin.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::ops::{RangeFrom, RangeTo};

use ethereum_types::H256;

use crate::block_id::BlockNumberOrTag;

/// Represents the target range of blocks for the filter.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FilterBlockOption {
	/// A range of blocks with optional from and to blocks.
	///
	/// Note: ranges are considered to be __inclusive__.
	BlockNumberRange {
		/// The block number or tag this filter should start at.
		from_block: Option<BlockNumberOrTag>,
		/// The block number or tag this filter should end at.
		to_block: Option<BlockNumberOrTag>,
	},
	/// The hash of the block if the filter only targets a single block.
	///
	/// See [EIP-234](https://eips.ethereum.org/EIPS/eip-234) for more details.
	BlockHashAt { block_hash: H256 },
}

impl Default for FilterBlockOption {
	fn default() -> Self {
		Self::BlockNumberRange {
			from_block: None,
			to_block: None,
		}
	}
}

impl FilterBlockOption {
	/// Sets the block number this range filter should start at.
	pub const fn from_block(self, block: BlockNumberOrTag) -> Self {
		let to_block = if let Self::BlockNumberRange { to_block, .. } = self {
			to_block
		} else {
			None
		};
		Self::BlockNumberRange {
			from_block: Some(block),
			to_block,
		}
	}

	/// Sets the block number this range filter should end at.
	pub const fn to_block(self, block: BlockNumberOrTag) -> Self {
		let from_block = if let Self::BlockNumberRange { from_block, .. } = self {
			from_block
		} else {
			None
		};
		Self::BlockNumberRange {
			from_block,
			to_block: Some(block),
		}
	}

	/// Pins the block hash this filter should target.
	pub const fn block_hash(block_hash: H256) -> Self {
		Self::BlockHashAt { block_hash }
	}
}

impl<T: Into<BlockNumberOrTag>> From<RangeFrom<T>> for FilterBlockOption {
	fn from(value: RangeFrom<T>) -> Self {
		let from_block = Some(value.start.into());
		let to_block = Some(BlockNumberOrTag::Latest);
		Self::BlockNumberRange {
			from_block,
			to_block,
		}
	}
}

impl<T: Into<BlockNumberOrTag>> From<RangeTo<T>> for FilterBlockOption {
	fn from(value: RangeTo<T>) -> Self {
		let from_block = Some(BlockNumberOrTag::Earliest);
		let to_block = Some(value.end.into());
		Self::BlockNumberRange {
			from_block,
			to_block,
		}
	}
}

impl From<H256> for FilterBlockOption {
	fn from(value: H256) -> Self {
		Self::BlockHashAt { block_hash: value }
	}
}
