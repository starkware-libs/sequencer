//! Bootstrap flow integration test.
//!
//! This test verifies that:
//! 1. Bootstrap infrastructure (BootstrapConfig, BootstrapAddresses) works correctly
//! 2. Nodes can be configured with bootstrap mode enabled
//! 3. Bootstrap transactions can be generated and accepted by gateway

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
/// 2. The gateway accepts them (with allow_bootstrap_txs enabled)
/// 3. The batcher processes them successfully
///
/// Note: This test uses pre-populated storage to focus on testing the
/// bootstrap transaction processing. Full "start from empty storage"
/// bootstrap testing is more complex and requires additional infrastructure.
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
