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
    key_to_last_insert_time: HashMap<K, Instant>,
    insertion_ordered_queue: VecDeque<(Instant, K)>,
    ttl: Duration,
}

impl<K> TimeCache<K>
where
    K: Eq + Hash + Clone,
{
    /// Create a new time cache with the specified TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            key_to_last_insert_time: HashMap::new(),
            insertion_ordered_queue: VecDeque::new(),
            ttl,
        }
    }

    /// Check if a key is in the cache. Entries past the TTL remain "contained" until the next
    /// [`insert_and_get_expired`](Self::insert_and_get_expired) call evicts them.
    pub fn contains(&self, key: &K) -> bool {
        self.key_to_last_insert_time.contains_key(key)
    }

    /// Insert a key into the cache with the current timestamp.
    ///
    /// This also performs cleanup of expired entries from the front of the queue.
    /// Returns the keys that were expired.
    pub fn insert_and_get_expired(&mut self, key: K) -> Vec<K> {
        let now = Instant::now();
        let expired_keys = self.evict_expired(now);
        self.key_to_last_insert_time.insert(key.clone(), now);
        self.insertion_ordered_queue.push_back((now, key));
        expired_keys
    }

    /// Return the number of entries currently in the cache. Includes entries past their TTL that
    /// haven't been lazily evicted yet.
    pub fn len(&self) -> usize {
        self.key_to_last_insert_time.len()
    }

    pub fn is_empty(&self) -> bool {
        self.key_to_last_insert_time.is_empty()
    }

    /// Evict expired entries from the front of the insertion-order queue. Return the keys that were
    /// expired.
    fn evict_expired(&mut self, now: Instant) -> Vec<K> {
        let mut expired_keys = Vec::new();
        while let Some(&(inserted_at, _)) = self.insertion_ordered_queue.front() {
            if now.duration_since(inserted_at) < self.ttl {
                break;
            }
            let (expired_at, key) = self.insertion_ordered_queue.pop_front().unwrap();
            // This key may have been re-inserted after the queued entry was created, giving it
            // a newer timestamp in the map. Only remove it if the map still holds the old
            // (expired) timestamp; otherwise the key is still alive under its later insertion.
            if self.key_to_last_insert_time.get(&key) == Some(&expired_at) {
                self.key_to_last_insert_time.remove(&key);
                expired_keys.push(key);
            }
        }
        expired_keys
    }
}
