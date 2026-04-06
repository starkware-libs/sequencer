use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    create_invoke_txs,
    end_to_end_flow,
    validate_tx_count,
    EndToEndFlowArgs,
    EndToEndTestScenario,
    ACCOUNT_ID_0,
};
use blockifier::bouncer::BouncerWeights;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::rpc_transaction::RpcTransaction;

const N_TXS: usize = 3;

/// Verifies that validation-only nodes actively vote in consensus.
/// With 1 proposer and 2 validation-only nodes (equal weight), the proposer alone cannot reach
/// quorum (needs > 1.5/3), so at least one validation-only node must vote for every block.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn validation_only_node_required_for_quorum() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::ValidationOnlyFlowTestWithRequiredQuorum,
            create_test_scenario(),
            BouncerWeights::default().proving_gas,
        )
        .instance_indices([20, 21, 22, 23])
        .n_validation_only_nodes(2),
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
