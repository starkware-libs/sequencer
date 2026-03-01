use std::time::Duration;

use super::time_cache::TimeCache;

const TTL_MS: u64 = 1000;
const TTL: Duration = Duration::from_millis(TTL_MS);
const THREE_QUARTER_TTL: Duration = Duration::from_millis(TTL_MS * 3 / 4);
const PAST_EXPIRATION: Duration = Duration::from_millis(TTL_MS + 50);
const KEY1: &str = "key1";
const KEY2: &str = "key2";
const KEY3: &str = "key3";

#[tokio::test]
async fn test_time_cache_basic() {
    tokio::time::pause();

    let mut cache = TimeCache::new(TTL);

    cache.insert(KEY1);
    assert!(cache.contains(&KEY1));
    assert!(!cache.contains(&KEY2));

    tokio::time::advance(PAST_EXPIRATION).await;
    assert!(!cache.contains(&KEY1));
}

#[tokio::test]
async fn test_time_cache_cleanup() {
    tokio::time::pause();

    let mut cache = TimeCache::new(TTL);

    cache.insert(KEY1);
    cache.insert(KEY2);
    assert!(cache.contains(&KEY1));
    assert!(cache.contains(&KEY2));

    tokio::time::advance(PAST_EXPIRATION).await;

    assert!(!cache.contains(&KEY1));
    assert!(!cache.contains(&KEY2));
    // Expired entries haven't been evicted yet (eviction is lazy, on insert).
    assert_eq!(cache.capacity(), 2);

    // Insert triggers cleanup of expired entries.
    cache.insert(KEY3);
    assert!(cache.contains(&KEY3));
    assert!(!cache.contains(&KEY1));
    assert!(!cache.contains(&KEY2));
    assert_eq!(cache.capacity(), 1);
}

#[tokio::test]
async fn test_time_cache_reinsert_refreshes_expiration() {
    tokio::time::pause();

    let mut cache = TimeCache::new(TTL);

    cache.insert(KEY1);

    tokio::time::advance(THREE_QUARTER_TTL).await;
    cache.insert(KEY1);

    tokio::time::advance(THREE_QUARTER_TTL).await;
    assert!(cache.contains(&KEY1));

    // Inserting KEY2 triggers eviction of the stale KEY1 queue entry; verify KEY1 survives
    // because its refreshed timestamp is still within the TTL.
    cache.insert(KEY2);
    assert!(cache.contains(&KEY1));
    assert!(cache.contains(&KEY2));
}
