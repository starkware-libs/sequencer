use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use rstest::{fixture, rstest};
use starknet_api::transaction::TransactionHash;
use starknet_mempool_integration_tests::integration_test_setup::IntegrationTestSetup;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    MultiAccountTransactionGenerator::new()
}

#[rstest]
#[ignore = "Gilad: There are structural issues with funding new accounts and this need surgery.
            Will fix soon. Once fixed, the test logic also need work, it's stale by now."]
#[tokio::test]
async fn test_end_to_end(mut tx_generator: MultiAccountTransactionGenerator) {
    // Setup.
    let accounts: Vec<_> = [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ]
    .into_iter()
    .map(|account| tx_generator.register_account_for_flow_test(account))
    .collect();

    let mock_running_system = IntegrationTestSetup::new_for_accounts(&accounts).await;

    let account0_deploy_nonce0 = &tx_generator.account_with_id(0).generate_default_deploy_account();
    let account0_invoke_nonce1 = tx_generator.account_with_id(0).generate_default_invoke();
    let account1_invoke_nonce0 = tx_generator.account_with_id(1).generate_default_invoke();
    let account0_invoke_nonce2 = tx_generator.account_with_id(0).generate_default_invoke();

    // Test.

    let account0_deploy_nonce0_tx_hash =
        mock_running_system.assert_add_tx_success(account0_deploy_nonce0).await;

    mock_running_system.assert_add_tx_success(&account0_invoke_nonce1).await;

    // FIXME: invoke with nonce0 shouldn't be possible, fix it, make this FAIL.
    let account1_invoke_nonce0_tx_hash =
        mock_running_system.assert_add_tx_success(&account1_invoke_nonce0).await;

    mock_running_system.assert_add_tx_success(&account0_invoke_nonce2).await;

    let mempool_txs = mock_running_system.get_txs(4).await;

    // Assert.

    // Only the transactions with nonce 0 should be returned from the mempool,
    // because we haven't merged queue-replenishment yet.
    let expected_tx_hashes_from_get_txs =
        [account1_invoke_nonce0_tx_hash, account0_deploy_nonce0_tx_hash];

    // This assert should be replaced with 4 once queue-replenishment is merged, also add a tx hole
    // at that point, and ensure the assert doesn't change due to that.
    assert_eq!(mempool_txs.len(), 2);
    let actual_tx_hashes: Vec<TransactionHash> =
        mempool_txs.iter().map(|tx| tx.tx_hash()).collect();
    assert_eq!(expected_tx_hashes_from_get_txs, *actual_tx_hashes);
}
