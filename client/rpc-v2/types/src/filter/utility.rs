// This file is part of Frontier.

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

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Union type for representing a single value or a list of values inside a filter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueOrArray<T> {
	/// A single value.
	Value(T),
	/// A list of values.
	Array(Vec<T>),
}

impl<T> From<T> for ValueOrArray<T> {
	fn from(value: T) -> Self {
		Self::Value(value)
	}
}

impl<T> From<Vec<T>> for ValueOrArray<T> {
	fn from(array: Vec<T>) -> Self {
		Self::Array(array)
	}
}

/// FilterSet is a set of values that will be used to filter addresses and topics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterSet<T: Eq + Ord>(BTreeSet<T>);

impl<T: Eq + Ord> From<T> for FilterSet<T> {
	fn from(value: T) -> Self {
		Self(BTreeSet::from([value]))
	}
}

impl<T: Eq + Ord> From<Vec<T>> for FilterSet<T> {
	fn from(value: Vec<T>) -> Self {
		Self(value.into_iter().collect())
	}
}

impl<T: Eq + Ord> From<ValueOrArray<T>> for FilterSet<T> {
	fn from(value: ValueOrArray<T>) -> Self {
		match value {
			ValueOrArray::Value(value) => value.into(),
			ValueOrArray::Array(array) => array.into(),
		}
	}
}

impl<T: Eq + Ord> From<ValueOrArray<Option<T>>> for FilterSet<T> {
	fn from(src: ValueOrArray<Option<T>>) -> Self {
		match src {
			ValueOrArray::Value(None) => Self(BTreeSet::new()),
			ValueOrArray::Value(Some(value)) => value.into(),
			ValueOrArray::Array(array) => {
				// If the array contains at least one `null` (i.e. None), as it's considered
				// a "wildcard" value, the whole filter should be treated as matching everything,
				// thus is empty.
				if array.contains(&None) {
					Self(BTreeSet::new())
				} else {
					// Otherwise, we flatten the array, knowing there are no `None` values
					array.into_iter().flatten().collect::<Vec<T>>().into()
				}
			}
		}
	}
}

impl<T: Clone + Eq + Ord> FilterSet<T> {
	/// Returns whether the filter is empty.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Returns a [`ValueOrArray`] inside an Option:
	///   - If the filter is empty, it returns `None`
	///   - If the filter has only 1 value, it returns the single value
	///   - Otherwise it returns an array of values
	pub fn to_value_or_array(&self) -> Option<ValueOrArray<T>> {
		let values_len = self.0.len();
		match values_len {
			0 => None,
			1 => Some(ValueOrArray::Value(
				self.0.iter().next().cloned().expect("at least one item"),
			)),
			_ => Some(ValueOrArray::Array(self.0.iter().cloned().collect())),
		}
	}
}
