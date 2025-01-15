use assert_matches::assert_matches;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::test_utils::l1_handler::executable_l1_handler_tx;
use starknet_api::{l1_handler_tx_args, tx_hash};
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::SessionState::{
    self,
    Propose as ProposeSession,
    Validate as ValidateSession,
};
use starknet_l1_provider_types::ValidationStatus;

use crate::bootstrapper::{Bootstrapper, CommitBlockBacklog};
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

macro_rules! bootstrapper {
    (backlog: [$($height:literal => [$($tx:literal),* $(,)*]),* $(,)*], catch_up: $catch:expr) => {{
        Bootstrapper {
            commit_block_backlog: vec![
                $(CommitBlockBacklog {
                    height: BlockNumber($height),
                    committed_txs: vec![$(tx_hash!($tx)),*]
                }),*
            ].into_iter().collect(),
            catch_up_height: BlockNumber($catch),
        }
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
    assert_eq!(l1_provider.get_txs(0, BlockNumber(0)).unwrap(), []);
    assert_eq!(l1_provider.get_txs(1, BlockNumber(0)).unwrap(), [txs[0].clone()]);
    assert_eq!(l1_provider.get_txs(3, BlockNumber(0)).unwrap(), txs[1..=2]);
    assert_eq!(l1_provider.get_txs(1, BlockNumber(0)).unwrap(), []);
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
        l1_provider.validate(tx_hash!(1), BlockNumber(0)).unwrap(),
        ValidationStatus::Validated
    );
    assert_eq!(
        l1_provider.validate(tx_hash!(2), BlockNumber(0)).unwrap(),
        ValidationStatus::AlreadyIncludedOnL2
    );
    assert_eq!(
        l1_provider.validate(tx_hash!(3), BlockNumber(0)).unwrap(),
        ValidationStatus::ConsumedOnL1OrUnknown
    );
    // Transaction wasn't deleted after the validation.
    assert_eq!(
        l1_provider.validate(tx_hash!(1), BlockNumber(0)).unwrap(),
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
        l1_provider.get_txs(1, BlockNumber(0)).unwrap_err(),
        L1ProviderError::OutOfSessionGetTransactions
    );

    assert_matches!(
        l1_provider.validate(tx_hash!(1), BlockNumber(0)).unwrap_err(),
        L1ProviderError::OutOfSessionValidate
    );
}

#[test]
fn proposal_start_multiple_proposals_same_height() {
    // Setup.
    let mut l1_provider =
        L1ProviderContentBuilder::new().with_state(ProviderState::Pending).build_into_l1_provider();

    // Test all single-height combinations.
    const SESSION_TYPES: [SessionState; 2] = [ProposeSession, ValidateSession];
    for (session_1, session_2) in SESSION_TYPES.into_iter().cartesian_product(SESSION_TYPES) {
        l1_provider.start_block(BlockNumber(0), session_1).unwrap();
        l1_provider.start_block(BlockNumber(0), session_2).unwrap();
    }
}

#[test]
fn test_commit_block_empty_block() {
    // Setup.
    let txs = [tx!(tx_hash: 1), tx!(tx_hash: 2)];
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs(txs.clone())
        .with_height(BlockNumber(10))
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Commit block during proposal.
    l1_provider.commit_block(&[], BlockNumber(10)).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs(txs)
        .with_height(BlockNumber(11))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
fn test_commit_block_during_proposal() {
    // Setup.
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx!(tx_hash: 1), tx!(tx_hash: 2), tx!(tx_hash: 3)])
        .with_height(BlockNumber(10))
        .with_state(ProviderState::Propose)
        .build_into_l1_provider();

    // Commit block during proposal.
    l1_provider.commit_block(&[tx_hash!(1)], BlockNumber(10)).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx!(tx_hash: 2), tx!(tx_hash: 3)])
        .with_height(BlockNumber(11))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Commit block during pending.
    l1_provider.commit_block(&[tx_hash!(2)], BlockNumber(11)).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx!(tx_hash: 3)])
        .with_height(BlockNumber(12))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Commit block during validate.
    l1_provider.state = ProviderState::Validate;
    l1_provider.commit_block(&[tx_hash!(3)], BlockNumber(12)).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([])
        .with_height(BlockNumber(13))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}

#[test]
fn test_commit_block_backlog() {
    // Setup.
    let initial_bootstrap_state = ProviderState::Bootstrap(bootstrapper!(
        backlog: [10 => [2], 11 => [4]],
        catch_up: 10
    ));
    let mut l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx!(tx_hash: 1), tx!(tx_hash: 2), tx!(tx_hash: 4)])
        .with_height(BlockNumber(8))
        .with_state(initial_bootstrap_state.clone())
        .build_into_l1_provider();

    // Test.
    // Commit height too low to affect backlog.
    l1_provider.commit_block(&[tx_hash!(1)], BlockNumber(8)).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([tx!(tx_hash: 2), tx!(tx_hash: 4)])
        .with_height(BlockNumber(9))
        .with_state(initial_bootstrap_state)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);

    // Backlog is consumed, bootstrapping complete.
    l1_provider.commit_block(&[], BlockNumber(9)).unwrap();
    let expected_l1_provider = L1ProviderContentBuilder::new()
        .with_txs([])
        .with_height(BlockNumber(12))
        .with_state(ProviderState::Pending)
        .build();
    expected_l1_provider.assert_eq(&l1_provider);
}
