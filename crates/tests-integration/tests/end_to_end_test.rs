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
    for account in [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ] {
        tx_generator.register_account_for_flow_test(account);
    }

    let mock_running_system = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    let account0_invoke_nonce1 = tx_generator.account_with_id(0).generate_invoke_with_tip(1);
    let account0_invoke_nonce2 = tx_generator.account_with_id(0).generate_invoke_with_tip(2);
    let account1_invoke_nonce1 = tx_generator.account_with_id(1).generate_invoke_with_tip(3);

    let account0_invoke_nonce1_tx_hash =
        mock_running_system.assert_add_tx_success(&account0_invoke_nonce1).await;

    let account1_invoke_nonce1_tx_hash =
        mock_running_system.assert_add_tx_success(&account1_invoke_nonce1).await;

    let account0_invoke_nonce2_tx_hash =
        mock_running_system.assert_add_tx_success(&account0_invoke_nonce2).await;

    // Test.
    let mempool_txs = mock_running_system.get_txs(4).await;

    // Assert.
    // account1_invoke_nonce1 precedes account0_invoke_nonce1 as its nonce is lower, despite the
    // higher tip of the latter. account1_invoke_nonce1 precedes account0_invoke_nonce1 as it
    // offers a higher tip, regardless of the nonce. Hence the expected tx order, regardless of
    // tx hashes, is: account1_invoke_nonce1, account0_invoke_nonce1, and account0_invoke_nonce2.
    let expected_tx_hashes_from_get_txs = [
        account1_invoke_nonce1_tx_hash,
        account0_invoke_nonce1_tx_hash,
        account0_invoke_nonce2_tx_hash,
    ];
    let actual_tx_hashes: Vec<TransactionHash> =
        mempool_txs.iter().map(|tx| tx.tx_hash()).collect();
    assert_eq!(expected_tx_hashes_from_get_txs, *actual_tx_hashes);
}
