use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use rstest::{fixture, rstest};
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
        FeatureContract::AccountWithLongValidate(CairoVersion::Cairo0), // account id: 0
        FeatureContract::AccountWithLongValidate(CairoVersion::Cairo0), // account id: 1
        FeatureContract::FaultyAccount(CairoVersion::Cairo1),           // account id: 2, faulty.
        FeatureContract::AccountWithLongValidate(CairoVersion::Cairo1), // account id: 3
    ] {
        tx_generator.register_account_for_flow_test(account);
    }

    let mock_running_system = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    let account0_nonce1 = tx_generator.account_with_id(0).generate_invoke_with_tip(1);
    let account0_nonce2 = tx_generator.account_with_id(0).generate_invoke_with_tip(2);
    let account1_nonce1 = tx_generator.account_with_id(1).generate_invoke_with_tip(3);

    // Only add nonce 2 for this account, nonce 1 will be added later.
    let account0_nonce2_tx_hash = mock_running_system.assert_add_tx_success(&account0_nonce2).await;
    let account1_nonce1_tx_hash = mock_running_system.assert_add_tx_success(&account1_nonce1).await;

    // Should only get one transaction.
    mock_running_system.get_txs(5).await;

    // Add the missing nonce 1 from the account.
    let account0_nonce1_tx_hash = mock_running_system.assert_add_tx_success(&account0_nonce1).await;

    let faulty_account_tx = tx_generator.account_with_id(2).generate_invoke_with_tip(5);
    let account0_nonce2_tx_hash = mock_running_system.assert_add_tx_error(&faulty_account_tx).await; // Add error type?

    let account3_nonce1 = tx_generator.account_with_id(1).generate_invoke_with_tip(5);
    let account3_nonce2 = tx_generator.account_with_id(1).generate_invoke_with_tip(5);
    let account3_nonce1_tx_hash = mock_running_system.assert_add_tx_success(&account3_nonce1).await;
    let account3_nonce2_tx_hash = mock_running_system.assert_add_tx_success(&account3_nonce2).await;

    // Should only get the last two transactions which have higher tip, even if the first
    // two transactions are now in correct nonce order.
    mock_running_system.get_txs(2).await;

    mock_running_system.trigger_commit_block_as_proposer(); // add mockbatcher different behavior?
    mock_running_system.assert_batch_created(
        1, // 0 is genesis.
        &[account1_nonce1_tx_hash, account3_nonce1_tx_hash, account3_nonce2_tx_hash],
    );

    let account0_nonce3 = tx_generator.account_with_id(1).generate_invoke_with_tip(5);
    let account0_nonce4 = tx_generator.account_with_id(1).generate_invoke_with_tip(1);
    let account0_nonce4_tx_hash = mock_running_system.assert_add_tx_success(&account0_nonce4).await;
    // First two transactions should now be retrieved in this new block, but not the recently added
    // one, since it's parent nonce transaction hasn't been added.
    mock_running_system.get_txs(3).await;

    mock_running_system.trigger_commit_block_as_proposer(); // add mockbatcher different behavior?
    mock_running_system.assert_batch_created(
        1, // 0 is genesis.
        &[account0_nonce1_tx_hash, account0_nonce2_tx_hash],
    );
    // TODO: Continue test
}
