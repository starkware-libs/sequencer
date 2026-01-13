use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    create_invoke_txs,
    end_to_end_flow,
    validate_tx_count,
    EndToEndFlowArgs,
    EndToEndTestScenario,
    ACCOUNT_ID_1,
};
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;

const N_TXS: usize = 15;

/// This test checks that at least one block is full.
/// The test uses 3 threads: 1 for the test's main thread and 2 for the sequencers.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn many_txs_fill_at_least_one_block() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::EndToEndFlowTestManyTxs,
            create_many_txs_scenario(),
            // TODO(GFI): pass here the number of txs instead.
            GasAmount(30000000),
        )
        .expecting_full_blocks(),
    )
    .await
}

fn create_many_txs_scenario() -> EndToEndTestScenario {
    EndToEndTestScenario {
        create_rpc_txs_fn: create_many_invoke_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: |tx_hashes| validate_tx_count(tx_hashes, N_TXS),
    }
}

/// Creates and sends more transactions than can fit in a block.
pub fn create_many_invoke_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    create_invoke_txs(tx_generator, ACCOUNT_ID_1, N_TXS)
}
