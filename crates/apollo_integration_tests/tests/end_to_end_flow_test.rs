use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    create_deploy_account_tx_and_invoke_tx,
    create_l1_to_l2_messages_args,
    ACCOUNT_ID_0,
    ACCOUNT_ID_1,
    UNDEPLOYED_ACCOUNT_ID,
};
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_base_layer::ethereum_base_layer_contract::L1ToL2MessageArgs;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::common::{end_to_end_flow, test_single_tx, TestScenario};

mod common;

#[tokio::test]
async fn test_end_to_end_flow() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTest,
        create_test_scenarios(),
        GasAmount(50000000), // Enough gas to cover all transactions in one not full block.
        false,
        false,
    )
    .await
}

pub fn create_test_scenarios() -> Vec<TestScenario> {
    vec![
        // This block should be the first to be tested, as the addition of L1 handler transaction
        // does not work smoothly with the current architecture of the test.
        // TODO(Arni): Fix this. Move the L1 handler to be not the first block.
        TestScenario {
            create_rpc_txs_fn: |_| vec![],
            create_l1_to_l2_messages_args_fn: create_l1_to_l2_message_args,
            test_tx_hashes_fn: test_single_tx,
        },
        TestScenario {
            create_rpc_txs_fn: create_multiple_account_txs,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_multiple_account_txs,
        },
        TestScenario {
            create_rpc_txs_fn: create_funding_txs,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_single_tx,
        },
        TestScenario {
            create_rpc_txs_fn: deploy_account_and_invoke,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_two_txs,
        },
        TestScenario {
            create_rpc_txs_fn: create_declare_tx,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_single_tx,
        },
    ]
}

fn create_l1_to_l2_message_args(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<L1ToL2MessageArgs> {
    const N_TXS: usize = 1;
    create_l1_to_l2_messages_args(tx_generator, N_TXS)
}

fn create_multiple_account_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    // Create RPC transactions.
    let account0_invoke_nonce1 =
        tx_generator.account_with_id_mut(ACCOUNT_ID_0).generate_trivial_rpc_invoke_tx(2);
    let account0_invoke_nonce2 =
        tx_generator.account_with_id_mut(ACCOUNT_ID_0).generate_trivial_rpc_invoke_tx(3);
    let account1_invoke_nonce1 =
        tx_generator.account_with_id_mut(ACCOUNT_ID_1).generate_trivial_rpc_invoke_tx(4);

    vec![account0_invoke_nonce1, account0_invoke_nonce2, account1_invoke_nonce1]
}

fn test_multiple_account_txs(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    // Return the transaction hashes in the order they should be given by the mempool:
    // Transactions from the same account are ordered by nonce; otherwise, higher tips are given
    // priority.
    assert!(
        tx_hashes.len() == 3,
        "Unexpected number of transactions sent in the test scenario. Found {} transactions",
        tx_hashes.len()
    );
    vec![tx_hashes[2], tx_hashes[0], tx_hashes[1]]
}

fn create_funding_txs(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    // TODO(yair): Register the undeployed account here instead of in the test setup
    // once funding is implemented.
    let undeployed_account = tx_generator.account_with_id(UNDEPLOYED_ACCOUNT_ID).account;
    assert!(tx_generator.undeployed_accounts().contains(&undeployed_account));

    let funding_tx =
        tx_generator.account_with_id_mut(ACCOUNT_ID_0).generate_transfer(&undeployed_account);
    vec![funding_tx]
}

/// Generates a deploy account transaction followed by an invoke transaction from the same deployed
/// account.
fn deploy_account_and_invoke(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    create_deploy_account_tx_and_invoke_tx(tx_generator, UNDEPLOYED_ACCOUNT_ID)
}

fn test_two_txs(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert_eq!(tx_hashes.len(), 2, "Expected two transactions");
    tx_hashes.to_vec()
}

fn create_declare_tx(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(ACCOUNT_ID_0);
    let declare_tx = account_tx_generator.generate_declare();
    vec![declare_tx]
}
