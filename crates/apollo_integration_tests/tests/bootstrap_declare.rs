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

/// Bootstrap declare txs are unique: they are sent from a special address and do not increment its
/// nonce. As a result, they are not removed from the mempool upon successful execution, and will
/// only be removed after being rejected during a subsequent attempt.
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
