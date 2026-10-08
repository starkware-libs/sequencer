use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
    serialize_duration_as_milliseconds,
    serialize_duration_as_seconds,
};
use serde::{Deserialize, Serialize};
use tokio_retry::strategy::ExponentialBackoff;

/// Configuration for the peer discovery system.
///
/// This struct contains all parameters needed to configure the discovery
/// behavior, including retry policies and timing intervals.
///
/// # Examples
///
/// ```rust
/// use std::time::Duration;
///
/// use apollo_network::discovery::{DiscoveryConfig, RetryConfig};
///
/// let config = DiscoveryConfig {
///     bootstrap_dial_retry_config: RetryConfig {
///         base_delay_millis: 100,
///         max_delay_seconds: Duration::from_secs(10),
///         factor: 2,
///         new_connection_stabilization_millis: Duration::from_millis(2000),
///     },
///     heartbeat_interval: Duration::from_millis(500),
/// };
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryConfig {
    /// Configuration for retrying failed bootstrap peer connections.
    pub bootstrap_dial_retry_config: RetryConfig,

    /// Interval between periodic discovery operations.
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub heartbeat_interval: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            bootstrap_dial_retry_config: RetryConfig::default(),
            heartbeat_interval: Duration::from_millis(100),
        }
    }
}

/// Configuration for exponential backoff retry logic.
///
/// This struct defines the parameters for the exponential backoff strategy
/// used when retrying failed operations, particularly bootstrap peer connections.
///
/// # Exponential Backoff Algorithm
///
/// The delay between retry attempts follows this pattern:
/// - 1st retry: `base_delay_millis**1 * factor`
/// - 2nd retry: `base_delay_millis**2 * factor`
/// - 3rd retry: `base_delay_millis**3 * factor`
/// - And so on, capped at `max_delay_seconds`
///
/// # Examples
///
/// ```rust
/// use std::time::Duration;
///
/// use apollo_network::discovery::RetryConfig;
///
/// // Aggressive retry (fast but more network usage)
/// let aggressive = RetryConfig {
///     base_delay_millis: 2,                          // double each time
///     max_delay_seconds: Duration::from_millis(100), // Cap at 0.1 seconds
///     factor: 7,                                     // start with 7ms
///     new_connection_stabilization_millis: Duration::from_millis(2000),
/// };
///
/// let mut strategy = aggressive.strategy();
/// assert_eq!(strategy.next(), Some(Duration::from_millis(14)));
/// assert_eq!(strategy.next(), Some(Duration::from_millis(28)));
/// assert_eq!(strategy.next(), Some(Duration::from_millis(56)));
/// assert_eq!(strategy.next(), Some(Duration::from_millis(100)));
/// ```
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    /// Base of the exponential backoff in milliseconds, this will be the delay before the first
    /// retry (the first delay after the first attempt)
    pub base_delay_millis: u64,

    /// Maximum delay of the exponential backoff.
    #[serde(
        deserialize_with = "deserialize_seconds_to_duration",
        serialize_with = "serialize_duration_as_seconds"
    )]
    pub max_delay_seconds: Duration,

    /// Multiplication factor for the exponential backoff.
    pub factor: u64,

    /// Milliseconds to wait on a new connection before treating it as stable. Redials within
    /// this window (e.g. from an immediately refused connection) use accumulated backoff.
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub new_connection_stabilization_millis: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            base_delay_millis: 2,
            max_delay_seconds: Duration::from_secs(5),
            factor: 5,
            new_connection_stabilization_millis: Duration::from_millis(2000),
        }
    }
}

impl RetryConfig {
    pub fn strategy(&self) -> ExponentialBackoff {
        ExponentialBackoff::from_millis(self.base_delay_millis)
            .max_delay(self.max_delay_seconds)
            .factor(self.factor)
    }
}
