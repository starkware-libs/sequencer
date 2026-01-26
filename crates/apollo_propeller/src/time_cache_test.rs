use std::time::Duration;

use super::time_cache::TimeCache;

#[tokio::test]
async fn test_time_cache_basic() {
    tokio::time::pause();

    let mut cache = TimeCache::new(Duration::from_millis(100));

    // Insert and check
    cache.insert("key1");
    assert!(cache.contains(&"key1"));
    assert!(!cache.contains(&"key2"));

    // Advance time past expiration
    tokio::time::advance(Duration::from_millis(150)).await;
    assert!(!cache.contains(&"key1"));
}

#[tokio::test]
async fn test_time_cache_cleanup() {
    tokio::time::pause();

    let mut cache = TimeCache::new(Duration::from_millis(100));

    cache.insert("key1");
    cache.insert("key2");
    assert!(cache.contains(&"key1"));
    assert!(cache.contains(&"key2"));

    // Advance time past expiration
    tokio::time::advance(Duration::from_millis(150)).await;

    // Expired entries don't match when checking
    assert!(!cache.contains(&"key1"));
    assert!(!cache.contains(&"key2"));

    // Insert triggers cleanup of expired entries
    cache.insert("key3");
    assert!(cache.contains(&"key3"));
    assert!(!cache.contains(&"key1"));
    assert!(!cache.contains(&"key2"));
}
