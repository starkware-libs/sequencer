use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    create_deploy_account_tx_and_invoke_tx,
    UNDEPLOYED_ACCOUNT_ID,
};
use blockifier::bouncer::BouncerWeights;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::common::{end_to_end_flow, validate_tx_count, EndToEndFlowArgs, TestScenario};

mod common;

// TODO(Meshi): Fail the test if no class have migrated.
// Uses end_to_end_flow with test identifier EndToEndFlowTest and instance indices [3, 4, 5].
/// Number of threads is 3 = Num of sequencer + 1 for the test thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn deploy_account_and_invoke_flow() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::EndToEndFlowTest,
            create_test_scenarios(),
            BouncerWeights::default().proving_gas,
        )
        .instance_indices([3, 4, 5]),
    )
    .await
}

fn create_test_scenarios() -> TestScenario {
    TestScenario {
        create_rpc_txs_fn: deploy_account_and_invoke,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: |tx_hashes| validate_tx_count(tx_hashes, 2),
    }
}

/// Generates a deploy account transaction followed by an invoke transaction from the same deployed
/// account.
fn deploy_account_and_invoke(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    create_deploy_account_tx_and_invoke_tx(tx_generator, UNDEPLOYED_ACCOUNT_ID)
}
