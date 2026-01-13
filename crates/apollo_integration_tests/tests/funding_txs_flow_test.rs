use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    end_to_end_flow,
    test_single_tx,
    EndToEndFlowArgs,
    EndToEndTestScenario,
    ACCOUNT_ID_0,
    UNDEPLOYED_ACCOUNT_ID,
};
use blockifier::bouncer::BouncerWeights;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::rpc_transaction::RpcTransaction;

// Uses end_to_end_flow with test identifier EndToEndFlowTest and instance indices [6, 7, 8].
/// Number of threads is 3 = Num of sequencer + 1 for the test thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn funding_txs_flow() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::EndToEndFlowTest,
            create_test_scenarios(),
            BouncerWeights::default().proving_gas,
        )
        .instance_indices([6, 7, 8])
        .proposal_margin_millis(100, 1000),
    )
    .await
}

fn create_test_scenarios() -> EndToEndTestScenario {
    EndToEndTestScenario {
        create_rpc_txs_fn: create_funding_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_single_tx,
    }
}

fn create_funding_txs(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    // TODO(yair): Register the undeployed account here instead of in the test setup
    // once funding is implemented.
    let undeployed_account = tx_generator.account_with_id(UNDEPLOYED_ACCOUNT_ID).account;
    assert!(tx_generator.undeployed_accounts().contains(&undeployed_account));

    let funding_tx =
        tx_generator.account_with_id_mut(ACCOUNT_ID_0).generate_transfer(&undeployed_account);
    vec![funding_tx]
}
