use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PeerManagerConfig {
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub malicious_timeout_seconds: Duration,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub unstable_timeout_millis: Duration,
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

impl SerializeConfig for PeerManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "malicious_timeout_seconds",
                &self.malicious_timeout_seconds.as_secs(),
                "The duration in seconds a peer is blacklisted after being marked as malicious.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "unstable_timeout_millis",
                &self.unstable_timeout_millis.as_millis(),
                "The duration in milliseconds a peer blacklisted after being reported as unstable.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
