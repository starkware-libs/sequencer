use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    end_to_end_flow,
    test_single_tx,
    EndToEndFlowArgs,
    EndToEndTestScenario,
    ACCOUNT_ID_0,
};
use blockifier::bouncer::BouncerWeights;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::rpc_transaction::RpcTransaction;

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn declare_tx_flow() {
    end_to_end_flow(EndToEndFlowArgs::new(
        TestIdentifier::EndToEndFlowTest,
        create_test_scenarios(),
        BouncerWeights::default().proving_gas,
    ))
    .await
}

fn create_test_scenarios() -> EndToEndTestScenario {
    EndToEndTestScenario {
        create_rpc_txs_fn: create_declare_tx,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_single_tx,
    }
}

fn create_declare_tx(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(ACCOUNT_ID_0);
    let declare_tx = account_tx_generator.generate_declare_of_contract_class();
    vec![declare_tx]
}
