pub mod bootstrapper;
pub mod communication;
pub mod l1_provider;
pub mod l1_scraper;
pub mod soft_delete_index_map;

pub(crate) mod transaction_manager;

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
use starknet_l1_provider_types::SessionState;
use validator::Validate;

use crate::bootstrapper::Bootstrapper;

/// Current state of the provider, where pending means: idle, between proposal/validation cycles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderState {
    Pending,
    Propose,
    Bootstrap(Bootstrapper),
    Validate,
}

impl ProviderState {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderState::Pending => "Pending",
            ProviderState::Propose => "Propose",
            ProviderState::Bootstrap(_) => "Bootstrap",
            ProviderState::Validate => "Validate",
        }
    }

    pub fn is_bootstrapping(&self) -> bool {
        if let ProviderState::Bootstrap { .. } = self {
            return true;
        }

        false
    }

    pub fn get_bootstrapper(&mut self) -> Option<&mut Bootstrapper> {
        if let ProviderState::Bootstrap(bootstrapper) = self {
            return Some(bootstrapper);
        }

        None
    }

    fn transition_to_pending(&self) -> ProviderState {
        assert!(
            !self.is_bootstrapping(),
            "Transitioning from bootstrapping should be done manually by the L1Provider."
        );
        ProviderState::Pending
    }
}

impl From<SessionState> for ProviderState {
    fn from(state: SessionState) -> Self {
        match state {
            SessionState::Propose => ProviderState::Propose,
            SessionState::Validate => ProviderState::Validate,
        }
    }
}

impl std::fmt::Display for ProviderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for ProviderState {
    fn default() -> Self {
        ProviderState::Bootstrap(Bootstrapper::default())
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
