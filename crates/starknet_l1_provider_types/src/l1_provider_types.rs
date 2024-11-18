use serde::{Deserialize, Serialize};

use crate::errors::L1ProviderError;

pub type L1ProviderResult<T> = Result<T, L1ProviderError>;

/// Current state of the provider, where pending means: idle, between proposal/validation cycles.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub enum ProviderState {
    #[default]
    Pending,
    Propose,
    Validate,
}

impl ProviderState {
    pub fn transition_to_propose(self) -> L1ProviderResult<Self> {
        match self {
            ProviderState::Pending => Ok(ProviderState::Propose),
            _ => Err(L1ProviderError::UnexpectedProviderStateTransition {
                from: self,
                to: ProviderState::Propose,
            }),
        }
    }

    fn _transition_to_validate(self) -> L1ProviderResult<Self> {
        todo!()
    }

    fn _transition_to_pending(self) -> L1ProviderResult<Self> {
        todo!()
    }

    pub fn as_str(&self) -> &str {
        match self {
            ProviderState::Pending => "Pending",
            ProviderState::Propose => "Propose",
            ProviderState::Validate => "Validate",
        }
    }
}

impl std::fmt::Display for ProviderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
