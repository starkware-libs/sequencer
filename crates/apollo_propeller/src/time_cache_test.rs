use std::time::Duration;

use super::time_cache::TimeCache;

const TTL_MS: u64 = 1000;
const TTL: Duration = Duration::from_millis(TTL_MS);
const THREE_QUARTER_TTL: Duration = Duration::from_millis(TTL_MS * 3 / 4);
const PAST_EXPIRATION: Duration = Duration::from_millis(TTL_MS + 50);
const KEY1: &str = "key1";
const KEY2: &str = "key2";
const KEY3: &str = "key3";
const KEY4: &str = "key4";

#[tokio::test]
async fn test_time_cache_basic() {
    tokio::time::pause();

    let mut cache = TimeCache::new(TTL);

    let expired = cache.insert_and_get_expired(KEY1);
    assert!(expired.is_empty());
    assert!(cache.contains(&KEY1));
    assert!(!cache.contains(&KEY2));

    // Past TTL but no insert has happened — entry remains contained until eviction runs.
    tokio::time::advance(PAST_EXPIRATION).await;
    assert!(cache.contains(&KEY1));

    // Inserting a new key triggers eviction of the expired entry.
    let expired = cache.insert_and_get_expired(KEY2);
    assert_eq!(expired, vec![KEY1]);
    assert!(!cache.contains(&KEY1));
    assert!(cache.contains(&KEY2));
}

#[tokio::test]
async fn test_time_cache_cleanup() {
    tokio::time::pause();

    let mut cache = TimeCache::new(TTL);

    assert!(cache.insert_and_get_expired(KEY1).is_empty());
    assert!(cache.insert_and_get_expired(KEY2).is_empty());
    assert!(cache.contains(&KEY1));
    assert!(cache.contains(&KEY2));

    tokio::time::advance(PAST_EXPIRATION).await;

    // Expired entries haven't been evicted yet (eviction is lazy, on insert) — they remain
    // contained until the next insert.
    assert!(cache.contains(&KEY1));
    assert!(cache.contains(&KEY2));
    assert_eq!(cache.len(), 2);

    // Insert triggers cleanup of expired entries.
    let mut expired = cache.insert_and_get_expired(KEY3);
    expired.sort();
    assert_eq!(expired, vec![KEY1, KEY2]);
    assert!(cache.contains(&KEY3));
    assert!(!cache.contains(&KEY1));
    assert!(!cache.contains(&KEY2));
    assert_eq!(cache.len(), 1);
}

#[tokio::test]
async fn test_time_cache_reinsert_refreshes_expiration() {
    tokio::time::pause();

    let mut cache = TimeCache::new(TTL);

    assert!(cache.insert_and_get_expired(KEY1).is_empty());

    tokio::time::advance(THREE_QUARTER_TTL).await;
    assert!(cache.insert_and_get_expired(KEY1).is_empty());

    tokio::time::advance(THREE_QUARTER_TTL).await;
    assert!(cache.contains(&KEY1));

    // Inserting KEY2 triggers eviction of the stale KEY1 queue entry; verify KEY1 survives
    // because its refreshed timestamp is still within the TTL, and is NOT among the expired keys.
    let expired = cache.insert_and_get_expired(KEY2);
    assert!(expired.is_empty());
    assert!(cache.contains(&KEY1));
    assert!(cache.contains(&KEY2));
}

#[tokio::test]
async fn test_reinserted_key_expires_after_refreshed_ttl() {
    tokio::time::pause();

    let mut cache = TimeCache::new(TTL);

    assert!(cache.insert_and_get_expired(KEY1).is_empty());
    assert!(cache.insert_and_get_expired(KEY2).is_empty());

    tokio::time::advance(THREE_QUARTER_TTL).await;
    // Re-insert KEY1, refreshing its TTL.
    // KEY2 is not re-inserted so it will expire after next advance.
    assert!(cache.insert_and_get_expired(KEY1).is_empty());

    // Advance past the original TTL — KEY2 expires, KEY1's refreshed entry is still alive.
    tokio::time::advance(THREE_QUARTER_TTL).await;

    // Insert KEY3 to trigger eviction. Only KEY2 should be expired.
    let expired = cache.insert_and_get_expired(KEY3);
    assert_eq!(expired, vec![KEY2]);
    assert!(cache.contains(&KEY1));
    assert!(!cache.contains(&KEY2));
    assert!(cache.contains(&KEY3));

    // Advance past KEY1's refreshed TTL.
    tokio::time::advance(THREE_QUARTER_TTL).await;

    // Insert again to trigger eviction of KEY1.
    let expired = cache.insert_and_get_expired(KEY4);
    assert_eq!(expired, vec![KEY1]);
    assert!(!cache.contains(&KEY1));
    assert!(cache.contains(&KEY3));
}
