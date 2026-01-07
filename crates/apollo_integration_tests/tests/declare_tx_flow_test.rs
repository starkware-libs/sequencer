use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::ACCOUNT_ID_0;
use blockifier::bouncer::BouncerWeights;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::common::{end_to_end_flow, test_single_tx, EndToEndFlowArgs, TestScenario};

mod common;

/// Number of threads is 3 = Num of sequencer + 1 for the test thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_declare_tx_flow() {
    end_to_end_flow(EndToEndFlowArgs::new(
        TestIdentifier::EndToEndFlowTest,
        create_test_scenarios(),
        BouncerWeights::default().proving_gas,
    ))
    .await
}

fn create_test_scenarios() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_declare_tx,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_single_tx,
    }]
}

fn create_declare_tx(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(ACCOUNT_ID_0);
    let declare_tx = account_tx_generator.generate_declare_of_contract_class();
    vec![declare_tx]
}
