use apollo_infra_utils::test_utils::TestIdentifier;
use common::{end_to_end_flow, test_single_tx, TestScenario};
use mempool_test_utils::starknet_api_test_utils::generate_bootstrap_declare;
use starknet_api::execution_resources::GasAmount;

mod common;

fn create_bootstrap_declare_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: |_| vec![generate_bootstrap_declare()],
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_single_tx,
    }]
}

#[tokio::test]
async fn many_txs() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestBootstrapDeclare,
        create_bootstrap_declare_scenario(),
        GasAmount(29000000),
        false,
        false,
    )
    .await
}
