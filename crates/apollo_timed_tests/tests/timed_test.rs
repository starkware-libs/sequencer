use std::thread;
use std::time::Duration;

use apollo_timed_tests::{timed_rstest, timed_rstest_tokio, timed_test, timed_tokio_test};

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

// Tests that are expected to fail on actual test content (not timeout)
#[timed_test]
#[should_panic(expected = "assertion")]
fn test_sync_fails_on_assertion() {
    assert_eq!(1, 2); // This will fail, not the timeout
}

#[timed_tokio_test]
#[should_panic(expected = "assertion")]
async fn test_async_fails_on_assertion() {
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(1, 2); // This will fail, not the timeout
}

#[timed_rstest]
#[case(1)]
#[case(2)]
#[should_panic(expected = "assertion")]
fn test_rstest_sync_fails_on_assertion(#[case] value: u32) {
    assert_eq!(value, 999); // This will fail, not the timeout
}

#[timed_rstest_tokio]
#[case(1)]
#[case(2)]
#[should_panic(expected = "assertion")]
async fn test_rstest_async_fails_on_assertion(#[case] value: u32) {
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(value, 999); // This will fail, not the timeout
}

// Tests that are expected to fail on timeout
#[timed_test(50)]
#[should_panic(expected = "exceeded time limit")]
fn test_sync_fails_on_timeout() {
    thread::sleep(Duration::from_millis(100)); // This will exceed the 50ms limit
}

#[timed_tokio_test(50)]
#[should_panic(expected = "exceeded time limit")]
async fn test_async_fails_on_timeout() {
    tokio::time::sleep(Duration::from_millis(100)).await; // This will exceed the 50ms limit
}

#[timed_rstest(50)]
#[case(1)]
#[case(2)]
#[should_panic(expected = "exceeded time limit")]
fn test_rstest_sync_fails_on_timeout(#[case] _value: u32) {
    thread::sleep(Duration::from_millis(100)); // This will exceed the 50ms limit
}

#[timed_rstest_tokio(50)]
#[case(1)]
#[case(2)]
#[should_panic(expected = "exceeded time limit")]
async fn test_rstest_async_fails_on_timeout(#[case] _value: u32) {
    tokio::time::sleep(Duration::from_millis(100)).await; // This will exceed the 50ms limit
}
