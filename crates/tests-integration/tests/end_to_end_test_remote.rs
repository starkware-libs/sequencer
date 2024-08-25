use std::net::{IpAddr, Ipv6Addr};

use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_integration_tests::integration_test_utils::{
    create_config_remote,
    setup_with_tx_generation,
};
use starknet_mempool_integration_tests::state_reader::spawn_test_rpc_state_reader;

#[tokio::test]
async fn test_end_to_end() {
    // Setup.
    let accounts = [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ];

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr = spawn_test_rpc_state_reader(accounts).await;

    let mempool_ip: IpAddr = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
    let config = create_config_remote(rpc_server_addr, mempool_ip, 10001, 3).await;

    let (mock_running_system, mut tx_generator) = setup_with_tx_generation(&accounts, config).await;

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
