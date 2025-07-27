use apollo_infra_utils::test_utils::TestIdentifier;
use mempool_test_utils::starknet_api_test_utils::generate_bootstrap_declare;
use starknet_api::execution_resources::GasAmount;

use crate::common::{end_to_end_flow, test_single_tx, TestScenario};

mod common;

fn create_bootstrap_declare_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: |_| vec![generate_bootstrap_declare()],
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_single_tx,
    }]
}

/// Bootstrap declare txs are unique, they don't have a nonce. After execution, the tx is not
/// removed from the mempool without being rejected because it doesn't have a nonce. So every
/// bootstrap declare tx is executed twice, once accepted and once rejected.
#[tokio::test]
async fn bootstrap_declare() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestBootstrapDeclare,
        create_bootstrap_declare_scenario(),
        GasAmount(29000000),
        false,
        true,
    )
    .await
}
