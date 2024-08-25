use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::transaction::TransactionHash;
use starknet_mempool_integration_tests::integration_test_setup::IntegrationTestSetup;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    MultiAccountTransactionGenerator::new()
}

#[rstest]
#[tokio::test]
async fn test_end_to_end(mut tx_generator: MultiAccountTransactionGenerator) {
    // Setup.
    let (deploy_account_0, account0_deploy_tx_nonce0) = tx_generator
        .new_account_default(FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1));

    let (deploy_account_1, account1_deploy_tx_nonce0) = tx_generator
        .new_account_default(FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0));

    let accounts = [deploy_account_0, deploy_account_1];
    let mock_running_system = IntegrationTestSetup::new_for_account_contracts(&accounts).await;

    let account0_invoke_nonce1 = tx_generator.account_with_id(0).generate_default_invoke();
    let account0_invoke_nonce2 = tx_generator.account_with_id(0).generate_default_invoke();

    // Test.

    let account0_deploy_nonce0_tx_hash =
        mock_running_system.assert_add_tx_success(&account0_deploy_tx_nonce0).await;

    let account1_invoke_nonce1_tx_hash =
        mock_running_system.assert_add_tx_success(&account0_invoke_nonce1).await;

    let account1_deploy_nonce0_tx_hash =
        mock_running_system.assert_add_tx_success(&account1_deploy_tx_nonce0).await;

    let account0_invoke_nonce1_tx_hash =
        mock_running_system.assert_add_tx_success(&account0_invoke_nonce2).await;

    let mempool_txs = mock_running_system.get_txs(4).await;

    // Assert.

    // Only the transactions with nonce 0 should be returned from the mempool,
    // because we haven't merged queue-replenishment yet.
    let expected_tx_hashes_from_get_txs = [
        account0_deploy_nonce0_tx_hash,
        account1_deploy_nonce0_tx_hash,
        account1_invoke_nonce1_tx_hash,
        account0_invoke_nonce1_tx_hash,
    ];

    assert_eq!(mempool_txs.len(), 4);
    let actual_tx_hashes: Vec<TransactionHash> =
        mempool_txs.iter().map(|tx| tx.tx_hash()).collect();
    assert_eq!(expected_tx_hashes_from_get_txs, *actual_tx_hashes);
}
