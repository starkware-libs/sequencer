use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::test_utils::l1_handler::executable_l1_handler_tx;
use starknet_api::{l1_handler_tx_args, tx_hash};
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::SessionState::{
    Propose as ProposeSession,
    Validate as ValidateSession,
};
use starknet_l1_provider_types::ValidationStatus;

use crate::test_utils::L1ProviderContentBuilder;
use crate::ProviderState;

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
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Test.
    assert_eq!(l1_provider.get_txs(0, BlockNumber(1)).unwrap(), []);
    assert_eq!(l1_provider.get_txs(1, BlockNumber(1)).unwrap(), [txs[0].clone()]);
    assert_eq!(l1_provider.get_txs(3, BlockNumber(1)).unwrap(), txs[1..=2]);
    assert_eq!(l1_provider.get_txs(1, BlockNumber(1)).unwrap(), []);
}

#[test]
fn validate_happy_flow() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx!(tx_hash: 1)])
        .with_committed([tx_hash!(2)])
        .with_state(ProviderState::Validate)
        .build_into_l1_provider();

    // Test.
    assert_eq!(
        l1_provider.validate(tx_hash!(1), BlockNumber(1)).unwrap(),
        ValidationStatus::Validated
    );
    assert_eq!(
        l1_provider.validate(tx_hash!(2), BlockNumber(1)).unwrap(),
        ValidationStatus::AlreadyIncludedOnL2
    );
    assert_eq!(
        l1_provider.validate(tx_hash!(3), BlockNumber(1)).unwrap(),
        ValidationStatus::ConsumedOnL1OrUnknown
    );
    // Transaction wasn't deleted after the validation.
    assert_eq!(
        l1_provider.validate(tx_hash!(1), BlockNumber(1)).unwrap(),
        ValidationStatus::AlreadyIncludedInPropsedBlock
    );
}

#[test]
fn pending_state_errors() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_state(ProviderState::Pending)
        .with_txs([tx!(tx_hash: 1)])
        .build_into_l1_provider();

    // Test.
    assert_matches!(
        l1_provider.get_txs(1, BlockNumber(1)).unwrap_err(),
        L1ProviderError::OutOfSessionGetTransactions
    );

    assert_matches!(
        l1_provider.validate(tx_hash!(1), BlockNumber(1)).unwrap_err(),
        L1ProviderError::OutOfSessionValidate
    );
}

#[test]
fn proposal_start_multiple_proposals_same_height() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_state(ProviderState::Pending).build_into_l1_provider();

    // Test.
    l1_provider.start_block(BlockNumber(1), ProposeSession).unwrap();
    l1_provider.start_block(BlockNumber(1), ValidateSession).unwrap();

    l1_provider.start_block(BlockNumber(1), ProposeSession).unwrap();
    l1_provider.start_block(BlockNumber(1), ProposeSession).unwrap();

    l1_provider.start_block(BlockNumber(1), ValidateSession).unwrap();
    l1_provider.start_block(BlockNumber(1), ValidateSession).unwrap();
}
