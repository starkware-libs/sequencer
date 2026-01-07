use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::create_l1_to_l2_messages_args;
use blockifier::bouncer::BouncerWeights;

use crate::common::{end_to_end_flow, test_single_tx, EndToEndFlowArgs, TestScenario};

mod common;

// Uses end_to_end_flow with test identifier EndToEndFlowTest and instance indices [9, 10, 11].
/// Number of threads is 3 = Num of sequencer + 1 for the test thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn l1_to_l2_message_flow() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::EndToEndFlowTest,
            create_test_scenarios(),
            BouncerWeights::default().proving_gas,
        )
        .instance_indices([9, 10, 11]),
    )
    .await
}

fn create_test_scenarios() -> TestScenario {
    TestScenario {
        create_rpc_txs_fn: |_| vec![],
        create_l1_to_l2_messages_args_fn: |tx_generator| {
            create_l1_to_l2_messages_args(tx_generator, 1, false)
        },
        test_tx_hashes_fn: test_single_tx,
    }
}
