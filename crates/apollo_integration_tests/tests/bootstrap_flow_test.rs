//! Bootstrap flow integration test.
//!
//! This test verifies that:
//! 1. Bootstrap infrastructure (BootstrapConfig, BootstrapAddresses) works correctly
//! 2. Bootstrap transactions are correctly generated
//! 3. The gateway accepts bootstrap transactions (with allow_bootstrap_txs enabled)

use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::bootstrap::{generate_bootstrap_transactions, BootstrapAddresses};
use apollo_integration_tests::utils::{
    end_to_end_flow,
    test_single_tx,
    EndToEndFlowArgs,
    EndToEndTestScenario,
};
use starknet_api::execution_resources::GasAmount;
use tracing::info;

/// Create a test scenario that sends a bootstrap declare transaction.
fn create_bootstrap_declare_scenario() -> EndToEndTestScenario {
    EndToEndTestScenario {
        create_rpc_txs_fn: |_| {
            // Use the first bootstrap transaction (account contract declare)
            let txs = generate_bootstrap_transactions();
            vec![txs.into_iter().next().expect("Should have bootstrap txs")]
        },
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_single_tx,
    }
}

/// Test that bootstrap transactions can be processed.
///
/// This test verifies that:
/// 1. Bootstrap transactions are correctly generated
/// 2. Bootstrap addresses are deterministic
/// 3. The gateway accepts bootstrap transactions (with allow_bootstrap_txs enabled)
/// 4. The batcher processes them successfully
///
/// Note: This test uses pre-populated storage. Full "start from empty storage"
/// bootstrap requires the blockifier to support executing transactions without
/// any pre-existing contracts (like fee tokens), which needs additional work.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn bootstrap_transactions_are_processed() {
    configure_tracing().await;

    // Verify bootstrap addresses are deterministic
    let addresses1 = BootstrapAddresses::get();
    let addresses2 = BootstrapAddresses::get();
    assert_eq!(
        addresses1.funded_account_address, addresses2.funded_account_address,
        "Bootstrap addresses should be deterministic"
    );
    assert_eq!(
        addresses1.eth_fee_token_address, addresses2.eth_fee_token_address,
        "ETH fee token address should be deterministic"
    );
    assert_eq!(
        addresses1.strk_fee_token_address, addresses2.strk_fee_token_address,
        "STRK fee token address should be deterministic"
    );

    info!("Running bootstrap declare transaction test");

    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::EndToEndFlowTestBootstrapDeclare,
            create_bootstrap_declare_scenario(),
            GasAmount(29000000),
        )
        .allow_bootstrap_txs(),
    )
    .await;

    info!("Bootstrap transaction test completed successfully");
}

/// Test that bootstrap transactions are generated correctly.
#[test]
fn bootstrap_transactions_are_generated() {
    let txs = generate_bootstrap_transactions();

    // Should have 5 transactions: 2 declares + deploy account + 2 invoke (fee token deploys)
    assert_eq!(txs.len(), 5, "Expected 5 bootstrap transactions");

    // First two should be declares
    assert!(
        matches!(&txs[0], starknet_api::rpc_transaction::RpcTransaction::Declare(_)),
        "First transaction should be a declare"
    );
    assert!(
        matches!(&txs[1], starknet_api::rpc_transaction::RpcTransaction::Declare(_)),
        "Second transaction should be a declare"
    );

    // Third should be deploy account
    assert!(
        matches!(&txs[2], starknet_api::rpc_transaction::RpcTransaction::DeployAccount(_)),
        "Third transaction should be deploy account"
    );

    // Last two should be invokes (for deploying fee tokens)
    assert!(
        matches!(&txs[3], starknet_api::rpc_transaction::RpcTransaction::Invoke(_)),
        "Fourth transaction should be an invoke"
    );
    assert!(
        matches!(&txs[4], starknet_api::rpc_transaction::RpcTransaction::Invoke(_)),
        "Fifth transaction should be an invoke"
    );
}
