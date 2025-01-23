use starknet_l1_provider_types::SessionState;

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

    pub fn transition_to_pending(&self) -> ProviderState {
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
