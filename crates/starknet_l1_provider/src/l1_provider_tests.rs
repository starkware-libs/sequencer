use assert_matches::assert_matches;

use crate::errors::L1ProviderError::UnexpectedProviderStateTransition;
use crate::{L1Provider, ProviderState};

#[test]
fn proposal_start_errors() {
    // Setup.
    let mut l1_provider = L1Provider::default();

    // Test.
    l1_provider.proposal_start().unwrap();
    assert_matches!(
        l1_provider.proposal_start().unwrap_err(),
        UnexpectedProviderStateTransition {
            from: ProviderState::Propose,
            to: ProviderState::Propose
        }
    );
}
