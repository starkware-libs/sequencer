use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{create_invoke_txs, ACCOUNT_ID_1};
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::common::{end_to_end_flow, TestScenario};

mod common;

/// This test checks that at least one block is full.
#[tokio::test]
async fn many_txs_fill_at_least_one_block() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestManyTxs,
        create_many_txs_scenario(),
        GasAmount(30000000),
        true,
        false,
    )
    .await
}

fn create_many_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_many_invoke_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_many_invoke_txs,
    }]
}

pub fn test_many_invoke_txs(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert!(
        tx_hashes.len() == 15,
        "Unexpected number of transactions sent in the test scenario. Found {} transactions",
        tx_hashes.len()
    );
    tx_hashes.to_vec()
}

/// Creates and sends more transactions than can fit in a block.
pub fn create_many_invoke_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    const N_TXS: usize = 15;
    create_invoke_txs(tx_generator, ACCOUNT_ID_1, N_TXS)
}
