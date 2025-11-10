use std::thread;
use std::time::Duration;

use apollo_proc_macros::{timed_rstest, timed_rstest_tokio, timed_test, timed_tokio_test};
use rstest::rstest;

// Test basic timed_test macro
#[timed_test]
fn test_fast_sync_test() {
    assert_eq!(1 + 1, 2);
}

#[timed_test(50)]
fn test_custom_time_limit() {
    thread::sleep(Duration::from_millis(10));
}

// Test timed_tokio_test macro
#[timed_tokio_test]
async fn test_fast_async_test() {
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(1 + 1, 2);
}

#[timed_tokio_test(100)]
async fn test_custom_async_time_limit() {
    tokio::time::sleep(Duration::from_millis(20)).await;
}

// Test timed_rstest macro
#[timed_rstest]
#[case(1)]
#[case(2)]
fn test_rstest_sync(#[case] value: u32) {
    assert!(value > 0);
}

#[timed_rstest(100)]
#[case(1)]
#[case(2)]
fn test_rstest_sync_custom_limit(#[case] value: u32) {
    thread::sleep(Duration::from_millis(10));
    assert!(value > 0);
}

// Test timed_rstest_tokio with async functions
#[timed_rstest_tokio]
#[case(1)]
#[case(2)]
async fn test_rstest_async(#[case] value: u32) {
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(value > 0);
}

#[timed_rstest_tokio(150)]
#[case(1)]
#[case(2)]
async fn test_rstest_async_custom_limit(#[case] value: u32) {
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(value > 0);
}
