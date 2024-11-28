use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, trace, warn};

#[cfg(test)]
#[path = "run_until_test.rs"]
mod run_until_test;

/// Struct to hold trace configuration
pub struct TraceConfig {
    pub level: LogLevel,
    pub message: String,
}

/// Enum for dynamically setting trace level
#[derive(Clone, Copy)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Runs an asynchronous function until a condition is met or max attempts are reached.
///
/// # Arguments
/// - `interval`: Time between each attempt (in milliseconds).
/// - `max_attempts`: Maximum number of attempts.
/// - `executable`: An asynchronous function to execute, which returns a value of type `T`.
/// - `condition`: A closure that takes a value of type `T` and returns `true` if the condition is
///   met.
/// - `trace_config`: Optional trace configuration for logging.
///
/// # Returns
/// - `Option<T>`: Returns `Some(value)` if the condition is met within the attempts, otherwise
///   `None`.
pub async fn run_until<T, F, C>(
    interval: u64,
    max_attempts: usize,
    mut executable: F,
    condition: C,
    trace_config: Option<TraceConfig>,
) -> Option<T>
where
    T: Clone + Send + std::fmt::Debug + 'static,
    F: FnMut() -> T + Send,
    C: Fn(&T) -> bool + Send + Sync,
{
    for attempt in 1..=max_attempts {
        let result = executable();

        // Log attempt message.
        if let Some(config) = &trace_config {
            let attempt_message = format!(
                "{}: Attempt {}/{}, Value {:?}",
                config.message, attempt, max_attempts, result
            );
            log_message(config.level, &attempt_message);
        }

        // Check if the condition is met.
        if condition(&result) {
            if let Some(config) = &trace_config {
                let success_message = format!(
                    "{}: Condition met on attempt {}/{}",
                    config.message, attempt, max_attempts
                );
                log_message(config.level, &success_message);
            }
            return Some(result);
        }

        // Wait for the interval before the next attempt.
        sleep(Duration::from_millis(interval)).await;
    }

    if let Some(config) = &trace_config {
        let failure_message =
            format!("{}: Condition not met after {} attempts.", config.message, max_attempts);
        log_message(config.level, &failure_message);
    }

    None
}

/// Logs a message at the specified level
fn log_message(level: LogLevel, message: &str) {
    match level {
        LogLevel::Trace => trace!("{}", message),
        LogLevel::Debug => debug!("{}", message),
        LogLevel::Info => info!("{}", message),
        LogLevel::Warn => warn!("{}", message),
        LogLevel::Error => error!("{}", message),
    }
}
