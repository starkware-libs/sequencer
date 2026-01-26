//! Time-based cache with automatic expiration.
//!
//! This module provides a simple cache that automatically expires entries after a specified TTL.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use tokio::time::Instant;

/// A cache that automatically expires entries after a specified time-to-live (TTL).
///
/// Entries are lazily cleaned up when checking for existence or inserting new entries.
#[derive(Debug)]
pub struct TimeCache<K> {
    entries: HashMap<K, Instant>,
    ttl: Duration,
}

impl<K> TimeCache<K>
where
    K: Eq + Hash,
{
    /// Create a new time cache with the specified TTL.
    pub fn new(ttl: Duration) -> Self {
        Self { entries: HashMap::new(), ttl }
    }

    /// Check if a key exists in the cache and has not expired.
    pub fn contains(&self, key: &K) -> bool {
        if let Some(&inserted_at) = self.entries.get(key) {
            // Check if the entry is still valid
            Instant::now().duration_since(inserted_at) < self.ttl
        } else {
            false
        }
    }

    /// Insert a key into the cache with the current timestamp.
    ///
    /// This also performs cleanup of expired entries.
    pub fn insert(&mut self, key: K) {
        let now = Instant::now();
        // Clean up expired entries
        self.entries.retain(|_, &mut inserted_at| now.duration_since(inserted_at) < self.ttl);
        self.entries.insert(key, now);
    }

    /// Get the number of entries in the cache (including potentially expired ones).
    ///
    /// Note: This count may include expired entries that haven't been cleaned up yet.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    ///
    /// Note: This may include expired entries that haven't been cleaned up yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
