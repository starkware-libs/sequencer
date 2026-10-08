use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
    serialize_duration_as_milliseconds,
    serialize_duration_as_seconds,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PeerManagerConfig {
    #[serde(
        deserialize_with = "deserialize_seconds_to_duration",
        serialize_with = "serialize_duration_as_seconds"
    )]
    pub(super) malicious_timeout_seconds: Duration,
    #[serde(
        deserialize_with = "deserialize_milliseconds_to_duration",
        serialize_with = "serialize_duration_as_milliseconds"
    )]
    pub(super) unstable_timeout_millis: Duration,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            // TODO(shahak): Increase this once we're in a non-trusted setup.
            malicious_timeout_seconds: Duration::from_secs(1),
            unstable_timeout_millis: Duration::from_millis(1000),
        }
    }
}
