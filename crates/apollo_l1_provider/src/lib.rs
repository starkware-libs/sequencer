pub mod bootstrapper;

pub mod communication;
pub mod config;
pub mod l1_provider;
pub mod l1_scraper;
pub mod metrics;

pub(crate) mod transaction_manager;
pub(crate) mod transaction_record;

#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

use apollo_l1_provider_types::SessionState;
use papyrus_base_layer::constants::{
    EventIdentifier,
    LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
};

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

pub const fn event_identifiers_to_track() -> &'static [EventIdentifier] {
    &[LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER, MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER]
}
