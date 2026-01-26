//! Time-based cache with automatic expiration.
//!
//! This module provides a simple cache that automatically expires entries after a specified TTL.
//! Cleanup is amortized O(1) per insert by exploiting the fact that all entries share the same
//! TTL, so entries in the insertion-order queue expire monotonically.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::time::Duration;

use tokio::time::Instant;

/// A cache that automatically expires entries after a specified time-to-live (TTL).
///
/// Entries are lazily cleaned up from the front of an insertion-order queue, making
/// cleanup amortized O(1) per insert instead of O(n).
#[derive(Debug)]
pub struct TimeCache<K: Clone> {
    entries: HashMap<K, Instant>,
    /// Insertion-ordered queue for efficient oldest-first eviction.
    order: VecDeque<(Instant, K)>,
    ttl: Duration,
}

impl<K> TimeCache<K>
where
    K: Eq + Hash + Clone,
{
    /// Create a new time cache with the specified TTL.
    pub fn new(ttl: Duration) -> Self {
        Self { entries: HashMap::new(), order: VecDeque::new(), ttl }
    }

    /// Check if a key exists in the cache and has not expired.
    pub fn contains(&self, key: &K) -> bool {
        match self.entries.get(key) {
            Some(&inserted_at) => Instant::now().duration_since(inserted_at) < self.ttl,
            None => false,
        }
    }

    /// Insert a key into the cache with the current timestamp.
    ///
    /// This also performs cleanup of expired entries from the front of the queue.
    pub fn insert(&mut self, key: K) {
        let now = Instant::now();
        self.evict_expired(now);
        self.entries.insert(key.clone(), now);
        self.order.push_back((now, key));
    }

    /// Return the number of non-expired entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache has no non-expired entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Evict expired entries from the front of the insertion-order queue.
    fn evict_expired(&mut self, now: Instant) {
        while let Some(&(inserted_at, _)) = self.order.front() {
            if now.duration_since(inserted_at) < self.ttl {
                break;
            }
            let (_, key) = self.order.pop_front().unwrap();
            // Only remove from the map if this queue entry is still the current one.
            // A re-inserted key will have a newer timestamp in the map.
            if let Some(&map_ts) = self.entries.get(&key) {
                if map_ts == inserted_at {
                    self.entries.remove(&key);
                }
            }
        }
    }
}
