use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::test_utils::l1_handler::executable_l1_handler_tx;
use starknet_api::transaction::TransactionHash;
use starknet_api::{l1_handler_tx_args, tx_hash};
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::ValidationStatus;

use crate::test_utils::L1ProviderContentBuilder;
use crate::L1Provider;
use crate::ProviderState::{Pending, Propose, Uninitialized, Validate};

macro_rules! tx {
    (tx_hash: $tx_hash:expr) => {{
        executable_l1_handler_tx(
            l1_handler_tx_args!(
                tx_hash: tx_hash!($tx_hash) , ..Default::default()
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
fn validate_happy_flow() {
    // Setup.
    let l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx!(tx_hash: 1)])
        .with_on_l2_awaiting_l1_consumption([tx_hash!(2)])
        .with_state(Validate)
        .build_into_l1_provider();

    // Test.
    assert_eq!(l1_provider.validate(tx_hash!(1)).unwrap(), ValidationStatus::Validated);
    assert_eq!(l1_provider.validate(tx_hash!(2)).unwrap(), ValidationStatus::AlreadyIncludedOnL2);
    assert_eq!(l1_provider.validate(tx_hash!(3)).unwrap(), ValidationStatus::ConsumedOnL1OrUnknown);
    // Transaction wasn't deleted after the validation.
    assert_eq!(l1_provider.validate(tx_hash!(1)).unwrap(), ValidationStatus::Validated);
}

#[test]
fn pending_state_errors() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_state(Pending)
        .with_txs([tx!(tx_hash: 1)])
        .build_into_l1_provider();

    // Test.
    assert_matches!(
        l1_provider.get_txs(1).unwrap_err(),
        L1ProviderError::GetTransactionsInPendingState
    );

    assert_matches!(
        l1_provider.validate(tx_hash!(1)).unwrap_err(),
        L1ProviderError::ValidateInPendingState
    );
}

#[test]
#[should_panic(expected = "Uninitialized L1 provider")]
fn uninitialized_get_txs() {
    let mut uninitialized_l1_provider = L1Provider::default();
    assert_eq!(uninitialized_l1_provider.state, Uninitialized);

    uninitialized_l1_provider.get_txs(1).unwrap();
}

#[test]
#[should_panic(expected = "Uninitialized L1 provider")]
fn uninitialized_validate() {
    let uninitialized_l1_provider = L1Provider::default();
    assert_eq!(uninitialized_l1_provider.state, Uninitialized);

    uninitialized_l1_provider.validate(TransactionHash::default()).unwrap();
}

#[test]
fn proposal_start_errors() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_state(Pending).build_into_l1_provider();
    // Test.
    l1_provider.proposal_start().unwrap();

    assert_eq!(
        l1_provider.proposal_start().unwrap_err(),
        L1ProviderError::unexpected_transition(Propose, Propose)
    );
    assert_eq!(
        l1_provider.validation_start().unwrap_err(),
        L1ProviderError::unexpected_transition(Propose, Validate)
    );
}

#[test]
fn validation_start_errors() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_state(Pending).build_into_l1_provider();

    // Test.
    l1_provider.validation_start().unwrap();

    assert_eq!(
        l1_provider.validation_start().unwrap_err(),
        L1ProviderError::unexpected_transition(Validate, Validate)
    );
    assert_eq!(
        l1_provider.proposal_start().unwrap_err(),
        L1ProviderError::unexpected_transition(Validate, Propose)
    );
}
