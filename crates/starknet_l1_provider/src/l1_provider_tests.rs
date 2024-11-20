use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::hash::StarkHash;
use starknet_api::l1_handler_tx_args;
use starknet_api::test_utils::l1_handler::executable_l1_handler_tx;
use starknet_api::transaction::TransactionHash;

use crate::errors::L1ProviderError;
use crate::test_utils::L1ProviderContentBuilder;
use crate::L1Provider;
use crate::ProviderState::{Propose, Validate};

macro_rules! tx {
    (tx_hash: $tx_hash:expr) => {{
        executable_l1_handler_tx(
            l1_handler_tx_args!(
                tx_hash: TransactionHash(StarkHash::from($tx_hash)) , ..Default::default()
            )
        )
    }};
}

#[test]
fn get_txs_happy_flow() {
    // Setup.
    let txs = [tx!(tx_hash: 0), tx!(tx_hash: 1), tx!(tx_hash: 2)];
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs(txs.clone())
        .with_state(Propose)
        .build_into_l1_provider();

    // Test.
    assert_eq!(l1_provider.get_txs(0).unwrap(), []);
    assert_eq!(l1_provider.get_txs(1).unwrap(), [txs[0].clone()]);
    assert_eq!(l1_provider.get_txs(3).unwrap(), txs[1..=2]);
    assert_eq!(l1_provider.get_txs(1).unwrap(), []);
}

#[test]
fn pending_state_errors() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_txs([tx!(tx_hash: 1)]).build_into_l1_provider();

    // Test.
    assert_matches!(
        l1_provider.get_txs(1).unwrap_err(),
        L1ProviderError::GetTransactionsInPendingState
    );
}

#[test]
fn proposal_start_errors() {
    // Setup.
    let mut l1_provider = L1Provider::default();

    // Test.
    l1_provider.proposal_start().unwrap();

    assert_matches!(
        l1_provider.proposal_start().unwrap_err(),
        L1ProviderError::UnexpectedProviderStateTransition { from: Propose, to: Propose }
    );
    assert_matches!(
        l1_provider.validation_start().unwrap_err(),
        L1ProviderError::UnexpectedProviderStateTransition { from: Propose, to: Validate }
    );
}

#[test]
fn validation_start_errors() {
    // Setup.
    let mut l1_provider = L1Provider::default();

    // Test.
    l1_provider.validation_start().unwrap();

    assert_matches!(
        l1_provider.validation_start().unwrap_err(),
        L1ProviderError::UnexpectedProviderStateTransition { from: Validate, to: Validate }
    );
    assert_matches!(
        l1_provider.proposal_start().unwrap_err(),
        L1ProviderError::UnexpectedProviderStateTransition { from: Validate, to: Propose }
    );
}
