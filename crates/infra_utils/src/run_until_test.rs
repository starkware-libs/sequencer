use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tokio::sync::Mutex;

use crate::run_until::run_until;

#[rstest]
#[tokio::test]
async fn test_run_until_condition_met() {
    let (inc_value_closure, get_value_closure, condition) = create_test_closures(3);

    // Run the function with a short interval and a maximum of 5 attempts.
    let result = run_until(100, 5, inc_value_closure, condition, None).await;

    // Assert that the condition was met and the returned value is correct.
    assert_eq!(result, Some(3));
    assert_eq!(get_value_closure().await, 3); // Counter should stop at 3 since the condition is met.
}

#[rstest]
#[tokio::test]
async fn test_run_until_condition_not_met() {
    let (inc_value_closure, get_value_closure, condition) = create_test_closures(3);

    // Test that it stops when the maximum attempts are exceeded without meeting the condition.
    let failed_result = run_until(100, 2, inc_value_closure, condition, None).await;

    // The condition is not met within 2 attempts, so the result should be None.
    assert_eq!(failed_result, None);
    assert_eq!(get_value_closure().await, 2); // Counter should stop at 2 because of max attempts.
}

// Type aliases to simplify the function signature
type AsyncFn = Box<dyn Fn() -> Pin<Box<dyn Future<Output = u32> + Send>> + Send + Sync>;
type SyncConditionFn = Box<dyn Fn(&u32) -> bool + Send + Sync>;

fn create_test_closures(condition_value: u32) -> (AsyncFn, AsyncFn, SyncConditionFn) {
    // Shared mutable state
    let counter = Arc::new(Mutex::new(0));

    // Async closure to increment the counter
    let increment_closure: Box<
        dyn Fn() -> Pin<Box<dyn Future<Output = u32> + Send>> + Send + Sync,
    > = {
        let counter = Arc::clone(&counter);
        Box::new(move || {
            let counter = Arc::clone(&counter);
            Box::pin(async move {
                let mut counter_lock = counter.lock().await;
                *counter_lock += 1;
                *counter_lock
            })
        })
    };

    // Async closure to get the current counter value
    let get_counter_value: Box<
        dyn Fn() -> Pin<Box<dyn Future<Output = u32> + Send>> + Send + Sync,
    > = {
        let counter = Arc::clone(&counter);
        Box::new(move || {
            let counter = Arc::clone(&counter);
            Box::pin(async move {
                let counter_lock = counter.lock().await;
                *counter_lock
            })
        })
    };

    // Synchronous condition closure
    let condition: Box<dyn Fn(&u32) -> bool + Send + Sync> =
        Box::new(move |&result: &u32| result >= condition_value);

    (increment_closure, get_counter_value, condition)
}
