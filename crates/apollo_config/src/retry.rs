//! A reusable configuration for retry-with-backoff mechanisms.

use serde::{Deserialize, Serialize};

/// A configuration for the retry mechanism.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    /// The initial waiting time in milliseconds.
    pub retry_base_millis: u64,
    /// The maximum waiting time in milliseconds.
    pub retry_max_delay_millis: u64,
    /// The maximum number of retries.
    pub max_retries: usize,
}
