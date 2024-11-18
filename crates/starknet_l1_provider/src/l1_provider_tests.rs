use assert_matches::assert_matches;
use indexmap::IndexMap;
use pretty_assertions::assert_eq;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::hash::StarkHash;
use starknet_api::l1_handler_tx_args;
use starknet_api::test_utils::l1_handler::executable_l1_handler_tx;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::errors::L1ProviderError::UnexpectedProviderStateTransition;
use starknet_l1_provider_types::l1_provider_types::ProviderState;

use crate::{L1Provider, TransactionManager};

#[macro_export]
macro_rules! tx {
    (tx_hash: $tx_hash:expr) => {{
        executable_l1_handler_tx(
            l1_handler_tx_args!(
                tx_hash: TransactionHash(StarkHash::from($tx_hash)) , ..Default::default()
            )
        )
    }};
}

// TODO: change to something more robust once we have more tests.
fn l1_provider(txs: Vec<L1HandlerTransaction>) -> L1Provider {
    let n_txs = txs.len();
    let awaiting_l2_inclusion: IndexMap<_, _> =
        txs.into_iter().map(|tx| (tx.tx_hash, tx)).collect();
    assert_eq!(
        awaiting_l2_inclusion.len(),
        n_txs,
        "Transactions given to this constructor should have unique hashes."
    );

    L1Provider {
        tx_manager: TransactionManager { txs: awaiting_l2_inclusion, ..Default::default() },
        ..Default::default()
    }
}

#[test]
fn get_txs_happy_flow() {
    // Setup.
    let txs = vec![tx!(tx_hash: 0), tx!(tx_hash: 1), tx!(tx_hash: 2)];
    let mut l1_provider = l1_provider(txs.clone());
    l1_provider.proposal_start().unwrap();

    // Test.
    assert_eq!(l1_provider.get_txs(0).unwrap(), vec![]);
    assert_eq!(l1_provider.get_txs(1).unwrap(), vec![txs[0].clone()]);
    assert_eq!(l1_provider.get_txs(3).unwrap(), txs[1..=2].to_vec());
    assert_eq!(l1_provider.get_txs(1).unwrap(), vec![]);
}

#[test]
fn pending_state_errors() {
    // Setup.
    let mut l1_provider = l1_provider(vec![tx!(tx_hash: 0)]);

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
        UnexpectedProviderStateTransition {
            from: ProviderState::Propose,
            to: ProviderState::Propose
        }
    );
}
