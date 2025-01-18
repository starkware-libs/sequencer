pub mod communication;

pub mod l1_provider;
pub mod l1_scraper;
pub(crate) mod transaction_manager;

mod soft_delete_index_map;

#[cfg(test)]
pub mod test_utils;

use std::collections::BTreeMap;
use std::time::Duration;

use papyrus_base_layer::constants::{
    EventIdentifier,
    CONSUMED_MESSAGE_TO_L1_EVENT_IDENTIFIER,
    LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
};
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
    fn transition_to_propose(self) -> L1ProviderResult<Self> {
        match self {
            ProviderState::Pending => Ok(ProviderState::Propose),
            _ => Err(L1ProviderError::unexpected_transition(self, ProviderState::Propose)),
        }
    }

    fn transition_to_validate(self) -> L1ProviderResult<Self> {
        match self {
            ProviderState::Pending => Ok(ProviderState::Validate),
            _ => Err(L1ProviderError::unexpected_transition(self, ProviderState::Validate)),
        }
    }

    fn _transition_to_pending(self) -> L1ProviderResult<Self> {
        todo!()
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

pub const fn event_identifiers_to_track() -> &'static [EventIdentifier] {
    &[
        LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
        CONSUMED_MESSAGE_TO_L1_EVENT_IDENTIFIER,
        MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
        MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
    ]
}
