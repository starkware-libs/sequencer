use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    create_non_generic_invoke_txs,
    ACCOUNT_ID_1,
    N_TXS_IN_NON_GENERIC_INVOKE_TXS,
};
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::common::{end_to_end_flow, TestScenario};

mod common;

#[tokio::test]
async fn all_custom_invoke_txs() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestCustomInvokeTxs,
        create_custom_invoke_txs_scenario(),
        GasAmount(40000000),
        false,
        false,
    )
    .await
}

fn create_custom_invoke_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_custom_invoke_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_many_invoke_txs,
    }]
}

pub fn test_many_invoke_txs(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert!(
        tx_hashes.len() == N_TXS_IN_NON_GENERIC_INVOKE_TXS,
        "Unexpected number of transactions sent in the test scenario. Found {} transactions",
        tx_hashes.len()
    );
    tx_hashes.to_vec()
}

/// Creates and sends more transactions than can fit in a block.
pub fn create_custom_invoke_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    create_non_generic_invoke_txs(tx_generator, ACCOUNT_ID_1)
}
