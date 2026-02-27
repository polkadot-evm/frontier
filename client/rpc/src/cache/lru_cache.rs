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

use scale_codec::Encode;
use schnellru::{LruMap, Unlimited};

pub struct LRUCacheByteLimited<K, V> {
	cache: LruMap<K, V, Unlimited>,
	max_size: u64,
	metrics: Option<LRUCacheByteLimitedMetrics>,
	size: u64,
}

impl<K: Eq + core::hash::Hash, V: Encode> LRUCacheByteLimited<K, V> {
	pub fn new(
		cache_name: &'static str,
		max_size: u64,
		prometheus_registry: Option<prometheus_endpoint::Registry>,
	) -> Self {
		let metrics = match prometheus_registry {
			Some(registry) => match LRUCacheByteLimitedMetrics::register(cache_name, &registry) {
				Ok(metrics) => Some(metrics),
				Err(e) => {
					log::error!(target: "eth-cache", "Failed to register metrics: {e:?}");
					None
				}
			},
			None => None,
		};

		Self {
			cache: LruMap::new(Unlimited),
			max_size,
			metrics,
			size: 0,
		}
	}
	pub fn get(&mut self, k: &K) -> Option<&V> {
		if let Some(v) = self.cache.get(k) {
			// Update metrics
			if let Some(metrics) = &self.metrics {
				metrics.hits.inc();
			}
			Some(v)
		} else {
			// Update metrics
			if let Some(metrics) = &self.metrics {
				metrics.miss.inc();
			}
			None
		}
	}
	pub fn put(&mut self, k: K, v: V) {
		// If the key is already present, remove it first so its old size is no longer
		// counted. Without this, re-inserting an existing key inflates `self.size` and
		// causes the eviction loop to throw out unrelated entries unnecessarily, making
		// the effective cache capacity much smaller than intended.
		if let Some(old_v) = self.cache.remove(&k) {
			self.size = self.size.saturating_sub(old_v.encoded_size() as u64);
		}

		self.size += v.encoded_size() as u64;

		while self.size > self.max_size {
			if let Some((_, evicted_v)) = self.cache.pop_oldest() {
				self.size = self.size.saturating_sub(evicted_v.encoded_size() as u64);
			} else {
				break;
			}
		}

		// Add entry in cache
		self.cache.insert(k, v);
		// Update metrics
		if let Some(metrics) = &self.metrics {
			metrics.size.set(self.size);
		}
	}
}

struct LRUCacheByteLimitedMetrics {
	hits: prometheus::IntCounter,
	miss: prometheus::IntCounter,
	size: prometheus_endpoint::Gauge<prometheus_endpoint::U64>,
}

impl LRUCacheByteLimitedMetrics {
	pub(crate) fn register(
		cache_name: &'static str,
		registry: &prometheus_endpoint::Registry,
	) -> Result<Self, prometheus_endpoint::PrometheusError> {
		Ok(Self {
			hits: prometheus_endpoint::register(
				prometheus::IntCounter::new(
					format!("frontier_eth_{cache_name}_hits"),
					format!("Hits of eth {cache_name} cache."),
				)?,
				registry,
			)?,
			miss: prometheus_endpoint::register(
				prometheus::IntCounter::new(
					format!("frontier_eth_{cache_name}_miss"),
					format!("Misses of eth {cache_name} cache."),
				)?,
				registry,
			)?,
			size: prometheus_endpoint::register(
				prometheus_endpoint::Gauge::new(
					format!("frontier_eth_{cache_name}_size"),
					format!("Size of eth {cache_name} data cache."),
				)?,
				registry,
			)?,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_size_limit() {
		let mut cache = LRUCacheByteLimited::new("name", 10, None);
		cache.put(0, "abcd");
		assert!(cache.get(&0).is_some());
		cache.put(1, "efghij");
		assert!(cache.get(&1).is_some());
		cache.put(2, "k");
		assert!(cache.get(&2).is_some());
		// Entry (0,  "abcd") should be deleted
		assert!(cache.get(&0).is_none());
		// Size should be 7 now, so we should be able to add a value of size 3
		cache.put(3, "lmn");
		assert!(cache.get(&3).is_some());
	}

	/// Regression test: re-inserting the same key must not inflate `size` and must not
	/// evict entries that would still fit within `max_size`.
	///
	/// Before the fix, `put(k, v2)` when `k` was already cached with `v1` would add
	/// `encoded_size(v2)` to `self.size` without subtracting `encoded_size(v1)`.  Over
	/// time this caused `self.size` to drift far above the actual data in the cache,
	/// leading to excessive LRU evictions and a degraded (nearly empty) cache.
	///
	/// We must replace the *newest* key (key 1). Replacing the oldest (key 0) would
	/// evict key 0 first, which accidentally "corrects" size; replacing key 1 evicts
	/// key 0 incorrectly when the bug exists.
	#[test]
	fn test_replace_same_key_does_not_inflate_size() {
		// max_size = 10 bytes, "abcd" = 4 bytes, "efgh" = 4 bytes.
		let mut cache = LRUCacheByteLimited::new("name", 10, None);

		// Fill with two entries that together fit. Order: 0=oldest, 1=newest.
		cache.put(0u32, "abcd"); // 4 bytes
		cache.put(1u32, "efgh"); // 4 bytes
		assert!(cache.get(&0).is_some());
		assert!(cache.get(&1).is_some());

		// Replace key 1 (newest) with a same-sized value. Before the fix, size grows
		// to 12, we evict oldest (key 0), and key 0 is incorrectly gone even though
		// both entries still fit.
		cache.put(1u32, "wxyz"); // 4 bytes replaces 4 bytes
		assert_eq!(
			cache.get(&1),
			Some(&"wxyz"),
			"key 1 should hold the new value"
		);
		assert!(
			cache.get(&0).is_some(),
			"key 0 must not be evicted: replacing a same-size value should not grow the cache"
		);
	}
}
