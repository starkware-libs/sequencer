use std::time::Duration;

use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    create_invoke_txs,
    end_to_end_flow,
    validate_tx_count,
    EndToEndFlowArgs,
    EndToEndTestScenario,
    NodeDescriptor,
    ACCOUNT_ID_0,
};
use blockifier::bouncer::BouncerWeights;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::rpc_transaction::RpcTransaction;

const N_TXS: usize = 3;

/// Verifies that a validation-only node can catch up via state sync after starting late.
///
/// Topology: 2 proposers + 1 validation-only node. With 3 stakers of weight 1 each and
/// `assume_no_malicious_validators = true` (which selects `HONEST_QUORUM`, 1/2), the 2
/// proposers reach quorum on their own and keep building blocks while the validation-only
/// node sleeps in its consensus `startup_delay`. When the validation-only node finally
/// joins consensus, it must catch up to the network's height via state sync —
/// `SequencerConsensusContext::try_sync` calls `state_sync.get_block` at the start of every
/// height to skip ahead when sync is already past the consensus height.
///
/// `state_sync.get_block` reads the `transaction_metadata` table, which is marked unused
/// under `StorageScope::StateOnly`. Validation-only nodes open state sync with
/// `StateOnly`, so `get_block` returns `ScopeError`, catch-up fails for every height the
/// validator is behind by, and the validator never reaches the network's height — the test
/// times out waiting for the validator's batcher height to match the proposers'.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn validation_only_node_catches_up_via_state_sync() {
    // Give the proposers a head start so they commit several blocks before the
    // validation-only node enters consensus and is forced to use state sync to catch up.
    // Default proposer `startup_delay` is 15s, so the validator joins ~15s after the
    // proposers begin building blocks.
    const VALIDATOR_STARTUP_DELAY: Duration = Duration::from_secs(30);
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::ValidationOnlyNodeNeededForQuorumTest,
            create_test_scenario(),
            BouncerWeights::default().proving_gas,
        )
        .node_descriptors(vec![
            NodeDescriptor::consolidated(),
            NodeDescriptor::consolidated(),
            NodeDescriptor::validation_only()
                .with_consensus_startup_delay(VALIDATOR_STARTUP_DELAY),
        ])
        .scenario_timeout(Duration::from_secs(120)),
    )
    .await
}

fn create_test_scenario() -> EndToEndTestScenario {
    EndToEndTestScenario {
        create_rpc_txs_fn: create_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: |tx_hashes| validate_tx_count(tx_hashes, N_TXS),
    }
}

fn create_txs(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    create_invoke_txs(tx_generator, ACCOUNT_ID_0, N_TXS)
}
