pub mod bootstrapper;

use apollo_config::converters::deserialize_float_seconds_to_duration;
pub mod communication;
pub mod l1_provider;
pub mod l1_scraper;
pub mod metrics;

pub(crate) mod transaction_manager;
pub(crate) mod transaction_record;

#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_l1_provider_types::SessionState;
use papyrus_base_layer::constants::{
    EventIdentifier,
    LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use validator::Validate;

use crate::bootstrapper::Bootstrapper;
use crate::transaction_manager::TransactionManagerConfig;

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

    /// Checks if the provider is in its uninitialized state. In this state, the provider has
    /// started, but its startup sequence, triggered via the initialization API, has not yet
    /// begun.
    pub fn uninitialized(&mut self) -> bool {
        self.get_bootstrapper().is_some_and(|bootstrapper| !bootstrapper.sync_started())
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Validate, PartialEq, Eq)]
pub struct L1ProviderConfig {
    /// In most cases this can remain None: the provider defaults to using the
    /// LastStateUpdate height at the L1 Height that the L1Scraper is initialized on.
    /// **WARNING**: Take care when setting this value, it must be no higher than the
    /// LastStateUpdate height at the L1 Height that the L1Scraper is initialized on.
    pub provider_startup_height_override: Option<BlockNumber>,
    /// In most cases this can remain None: the provider defaults to using the sync height at
    /// startup.
    pub bootstrap_catch_up_height_override: Option<BlockNumber>,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub startup_sync_sleep_retry_interval_seconds: Duration,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub l1_handler_cancellation_timelock_seconds: Duration,
    #[serde(deserialize_with = "deserialize_float_seconds_to_duration")]
    pub new_l1_handler_cooldown_seconds: Duration,
}

impl Default for L1ProviderConfig {
    fn default() -> Self {
        Self {
            provider_startup_height_override: None,
            bootstrap_catch_up_height_override: None,
            startup_sync_sleep_retry_interval_seconds: Duration::from_secs(2),
            l1_handler_cancellation_timelock_seconds: Duration::from_secs(5 * 60),
            new_l1_handler_cooldown_seconds: Duration::from_secs(4 * 60 + 15),
        }
    }
}

impl From<L1ProviderConfig> for TransactionManagerConfig {
    fn from(config: L1ProviderConfig) -> Self {
        TransactionManagerConfig {
            new_l1_handler_tx_cooldown_secs: config.new_l1_handler_cooldown_seconds,
            l1_handler_cancellation_timelock_seconds: config
                .l1_handler_cancellation_timelock_seconds,
        }
    }
}

impl SerializeConfig for L1ProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from([
            ser_param(
                "startup_sync_sleep_retry_interval_seconds",
                &self.startup_sync_sleep_retry_interval_seconds.as_secs(),
                "Interval in seconds between each retry of syncing with L2 during startup.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_handler_cancellation_timelock_seconds",
                &self.l1_handler_cancellation_timelock_seconds.as_secs(),
                "How long to allow a transaction requested for cancellation to be validated \
                 against (proposals are banned upon receiving a cancellation request).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "new_l1_handler_cooldown_seconds",
                &self.new_l1_handler_cooldown_seconds.as_secs(),
                "How long to wait before allowing new L1 handler transactions to be proposed \
                 (validation is available immediately).",
                ParamPrivacyInput::Public,
            ),
        ]);

        dump.extend(ser_optional_param(
            &self.provider_startup_height_override,
            Default::default(),
            "provider_startup_height_override",
            "Override height at which the provider should start",
            ParamPrivacyInput::Public,
        ));
        dump.extend(ser_optional_param(
            &self.bootstrap_catch_up_height_override,
            Default::default(),
            "bootstrap_catch_up_height_override",
            "Override height at which the provider should catch up to the bootstrapper.",
            ParamPrivacyInput::Public,
        ));
        dump
    }
}

pub const fn event_identifiers_to_track() -> &'static [EventIdentifier] {
    &[LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER, MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER]
}
