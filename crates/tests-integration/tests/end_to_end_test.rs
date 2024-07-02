use blockifier::test_utils::CairoVersion;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_integration_tests::integration_test_setup::IntegrationTestSetup;
use test_utils::starknet_api_test_utils::{deploy_account_tx, invoke_tx};

#[tokio::test]
async fn test_end_to_end() {
    let mut mock_running_system = IntegrationTestSetup::new(1).await;

    let mut expected_tx_hashes = Vec::new();
    expected_tx_hashes
        .push(mock_running_system.assert_add_tx_success(&invoke_tx(CairoVersion::Cairo0)).await);
    expected_tx_hashes
        .push(mock_running_system.assert_add_tx_success(&invoke_tx(CairoVersion::Cairo1)).await);
    expected_tx_hashes.push(mock_running_system.assert_add_tx_success(&deploy_account_tx()).await);

    let mempool_txs = mock_running_system.get_txs(4).await;
    assert_eq!(mempool_txs.len(), 3);
    let mut actual_tx_hashes: Vec<TransactionHash> =
        mempool_txs.iter().map(|tx| tx.tx_hash).collect();
    actual_tx_hashes.sort();
    expected_tx_hashes.sort();
    assert_eq!(expected_tx_hashes, actual_tx_hashes);
}
