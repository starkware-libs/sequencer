use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
};
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
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
/// use apollo_network_config::discovery::{DiscoveryConfig, RetryConfig};
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
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
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

impl SerializeConfig for DiscoveryConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([ser_param(
            "heartbeat_interval",
            &self.heartbeat_interval.as_millis(),
            "The interval between each discovery (Kademlia) query in milliseconds.",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut prepend_sub_config_name(
            self.bootstrap_dial_retry_config.dump(),
            "bootstrap_dial_retry_config",
        ));
        dump
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
/// use apollo_network_config::discovery::RetryConfig;
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
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub max_delay_seconds: Duration,

    /// Multiplication factor for the exponential backoff.
    pub factor: u64,

    /// Milliseconds to wait on a new connection before treating it as stable. Redials within
    /// this window (e.g. from an immediately refused connection) use accumulated backoff.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
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

impl SerializeConfig for RetryConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "base_delay_millis",
                &self.base_delay_millis,
                "The base delay in milliseconds for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_delay_seconds",
                &self.max_delay_seconds.as_secs(),
                "The maximum delay in seconds for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "factor",
                &self.factor,
                "The factor for the exponential backoff strategy.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "new_connection_stabilization_millis",
                &self.new_connection_stabilization_millis.as_millis(),
                "Milliseconds to wait on a new connection before treating it as stable.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl RetryConfig {
    pub fn strategy(&self) -> ExponentialBackoff {
        ExponentialBackoff::from_millis(self.base_delay_millis)
            .max_delay(self.max_delay_seconds)
            .factor(self.factor)
    }
}
