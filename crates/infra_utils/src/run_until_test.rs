use pretty_assertions::assert_eq;
use rstest::rstest;

use crate::run_until::run_until;

#[rstest]
#[tokio::test]
async fn test_run_until_condition_met() {
    // Mock executable that increments a counter.
    let mut counter = 0;
    let mock_executable = || {
        counter += 1;
        counter
    };

    // Condition: stop when the counter reaches 3.
    let condition = |&result: &i32| result >= 3;

    // Run the function with a short interval and a maximum of 5 attempts.
    let result = run_until(100, 5, mock_executable, condition, None).await;

    // Assert that the condition was met and the returned value is correct.
    assert_eq!(result, Some(3));
    assert_eq!(counter, 3); // Counter should stop at 3 since the condition is met.
}

#[rstest]
#[tokio::test]
async fn test_run_until_condition_not_met() {
    // Mock executable that increments a counter.
    let mut counter = 0;
    let mock_executable = || {
        counter += 1;
        counter
    };

    // Condition: stop when the counter reaches 3.
    let condition = |&result: &i32| result >= 3;

    // Test that it stops when the maximum attempts are exceeded without meeting the condition.
    let failed_result = run_until(100, 2, mock_executable, condition, None).await;

    // The condition is not met within 2 attempts, so the result should be None.
    assert_eq!(failed_result, None);
    assert_eq!(counter, 2); // Counter should stop at 2 because of max attempts.
}
