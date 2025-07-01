use std::future::Future;

use tokio::time::{sleep, Duration};

use crate::tracing::CustomLogger;

#[cfg(test)]
#[path = "run_until_test.rs"]
mod run_until_test;

/// Runs an asynchronous function until a condition is met or max attempts are reached.
///
/// # Arguments
/// - `interval`: Time between each attempt (in milliseconds).
/// - `max_attempts`: Maximum number of attempts.
/// - `executable`: An asynchronous function to execute, which returns a future type `T` value.
/// - `condition`: A closure that takes a value of type `T` and returns `true` if the condition is
///   met.
/// - `logger`: Optional trace logger.
///
/// # Returns
/// - `Option<T>`: Returns `Some(value)` if the condition is met within the attempts, otherwise
///   `None`.
pub async fn run_until<T, F, C, Fut>(
    interval: u64,
    max_attempts: usize,
    mut executable: F,
    condition: C,
    logger: Option<CustomLogger>,
) -> Option<T>
where
    T: Clone + Send + std::fmt::Debug + 'static,
    F: FnMut() -> Fut,
    Fut: Future<Output = T>,
    C: Fn(&T) -> bool + Send + Sync,
{
    for attempt in 1..=max_attempts {
        let result = executable().await;

        // Log attempt message.
        if let Some(config) = &logger {
            let attempt_message = format!("Attempt {attempt}/{max_attempts}, Value {result:?}");
            config.log_message(&attempt_message);
        }

        // Check if the condition is met.
        if condition(&result) {
            if let Some(config) = &logger {
                let success_message = format!("Condition met on attempt {attempt}/{max_attempts}");
                config.log_message(&success_message);
            }
            return Some(result);
        }

        // Wait for the interval before the next attempt.
        sleep(Duration::from_millis(interval)).await;
    }

    if let Some(config) = &logger {
        let failure_message =
            format!("Condition not met after the maximum number of {max_attempts} attempts.");
        config.log_message(&failure_message);
    }

    None
}
