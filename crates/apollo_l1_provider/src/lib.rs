pub mod bootstrapper;

pub mod communication;
pub mod l1_provider;
pub mod l1_scraper;
pub mod metrics;

pub(crate) mod transaction_manager;
pub(crate) mod transaction_record;

#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

pub use apollo_l1_provider_config::config::L1ProviderConfig;
use apollo_l1_provider_types::SessionState;
use papyrus_base_layer::constants::{
    EventIdentifier,
    CONSUMED_MESSAGE_TO_L1_EVENT_IDENTIFIER,
    LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
};

use crate::transaction_manager::TransactionManagerConfig;

/// Current state of the provider, where pending means: idle, between proposal/validation cycles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderState {
    /// Provider is not read for proposing or validating. Use  start_block to transition to Propose
    /// or Validate.
    Pending,
    /// Provider is ready for proposing. Use commit_block to finish and return to Pending.
    Propose,
    /// Provider is catching up using sync. Only happens on startup.
    Bootstrap,
    /// Provider is ready for validating. Use validate to validate a transaction.
    Validate,
}

impl ProviderState {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderState::Pending => "Pending",
            ProviderState::Propose => "Propose",
            ProviderState::Bootstrap => "Bootstrap",
            ProviderState::Validate => "Validate",
        }
    }

    pub fn is_bootstrapping(&self) -> bool {
        if let ProviderState::Bootstrap { .. } = self {
            return true;
        }

        false
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

// TODO(Nadin): Move to the l1 provider config crate.
impl From<L1ProviderConfig> for TransactionManagerConfig {
    fn from(config: L1ProviderConfig) -> Self {
        TransactionManagerConfig {
            new_l1_handler_tx_cooldown_secs: config.new_l1_handler_cooldown_seconds,
            l1_handler_cancellation_timelock_seconds: config
                .l1_handler_cancellation_timelock_seconds,
            l1_handler_consumption_timelock_seconds: config.l1_handler_consumption_timelock_seconds,
        }
    }
}

pub const fn event_identifiers_to_track() -> &'static [EventIdentifier] {
    &[
        LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
        MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
        CONSUMED_MESSAGE_TO_L1_EVENT_IDENTIFIER,
    ]
}
