use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::create_l1_to_l2_messages_args;
use blockifier::bouncer::BouncerWeights;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::transaction::L1HandlerTransaction;

use crate::common::{end_to_end_flow, test_single_tx, EndToEndFlowArgs, TestScenario};

mod common;

/// Number of threads is 3 = Num of sequencer + 1 for the test thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn reverted_l1_handler_tx_flow() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::RevertedL1HandlerTx,
            create_test_scenarios(),
            BouncerWeights::default().proving_gas,
        )
        .expecting_reverted_transactions(),
    )
    .await
}

fn create_test_scenarios() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: |_| vec![],
        create_l1_to_l2_messages_args_fn: create_l1_to_l2_reverted_message_args,
        test_tx_hashes_fn: test_single_tx,
    }]
}

fn create_l1_to_l2_reverted_message_args(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<L1HandlerTransaction> {
    const N_TXS: usize = 1;
    const SHOULD_REVERT: bool = true;
    create_l1_to_l2_messages_args(tx_generator, N_TXS, SHOULD_REVERT)
}
