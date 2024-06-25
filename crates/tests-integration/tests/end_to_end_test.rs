use blockifier::test_utils::CairoVersion;
use starknet_gateway::starknet_api_test_utils::invoke_tx;
use starknet_mempool_integration_tests::integration_test_setup::IntegrationTestSetup;

#[tokio::test]
async fn test_end_to_end() {
    let mut mock_running_system = IntegrationTestSetup::new(1).await;

    let expected_tx_hash =
        mock_running_system.assert_add_tx_success(&invoke_tx(CairoVersion::Cairo1)).await;

    let mempool_txs = mock_running_system.get_txs(2).await;
    assert_eq!(mempool_txs.len(), 1);
    assert_eq!(mempool_txs[0].tx_hash, expected_tx_hash);
}
