pub mod communication;
pub mod errors;
pub mod l1_provider;
pub(crate) mod staged_removal_index_map;
pub(crate) mod transaction_manager;

#[cfg(test)]
pub mod test_utils;

use std::collections::BTreeMap;
use std::time::Duration;

use papyrus_config::converters::deserialize_milliseconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::L1ProviderResult;
use validator::Validate;

#[cfg(test)]
#[path = "l1_provider_tests.rs"]
pub mod l1_provider_tests;

/// Current state of the provider, where pending means: idle, between proposal/validation cycles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProviderState {
    Pending,
    Propose,
    #[default]
    Uninitialized,
    Validate,
}

impl ProviderState {
    pub fn try_into_new_state(self, new_state: ProviderState) -> L1ProviderResult<ProviderState> {
        if new_state == ProviderState::Uninitialized {
            return Err(L1ProviderError::unexpected_transition(self, new_state));
        }

        Ok(new_state)
    }

    pub fn as_str(&self) -> &str {
        match self {
            ProviderState::Pending => "Pending",
            ProviderState::Propose => "Propose",
            ProviderState::Uninitialized => "Uninitialized",
            ProviderState::Validate => "Validate",
        }
    }
}

impl std::fmt::Display for ProviderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1ProviderConfig {
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub _poll_interval: Duration,
}

impl SerializeConfig for L1ProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "_poll_interval",
            &Duration::from_millis(100).as_millis(),
            "Interval in milliseconds between each scraping attempt of L1.",
            ParamPrivacyInput::Public,
        )])
    }
}
