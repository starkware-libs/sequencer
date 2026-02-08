//! Integration test for bootstrap flow with empty storage.
//!
//! This test verifies the FULL end-to-end bootstrap flow:
//! 1. Start sequencer nodes with empty storage (no pre-populated accounts)
//! 2. Nodes automatically run bootstrap phase before consensus starts
//! 3. Bootstrap transactions create fee tokens and fund an account
//! 4. Verify the funded account has the expected balances
//!
//! This differs from `integration_test_bootstrap_flow.rs` which tests infrastructure
//! components in isolation. This test runs actual sequencer nodes.

use apollo_integration_tests::bootstrap::{get_erc20_balance, BootstrapAddresses};
use apollo_integration_tests::flow_test_setup::FlowTestSetup;
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_storage::open_storage;
use starknet_api::block::BlockNumber;
use starknet_api::execution_resources::GasAmount;
use tracing::info;

/// Interval and attempts for waiting for nodes to be alive.
const AWAIT_ALIVE_INTERVAL_MS: u64 = 5000;
const AWAIT_ALIVE_ATTEMPTS: usize = 50;

/// The required balance that bootstrap must achieve.
/// This matches the value in BootstrapConfig.
const REQUIRED_BALANCE: u128 = 1_000_000_000_000_000_000_000_000_000; // 10^27

#[tokio::main]
async fn main() {
    integration_test_setup("bootstrap_empty_storage").await;

    info!("Starting bootstrap empty storage flow integration test");
    info!("This test verifies the full end-to-end bootstrap flow with running nodes");

    // ==========================================================================
    // Step 1: Create nodes with empty storage
    // ==========================================================================
    info!("\n--- Step 1: Creating nodes with empty storage ---");

    let test_setup = FlowTestSetup::new_for_bootstrap(
        0,                          // test_unique_index
        GasAmount(100_000_000_000), // block_max_capacity_gas
        [0, 1, 2],                  // instance_indices
    )
    .await;

    info!("Created FlowTestSetup with empty storage for bootstrap");
    info!("  - sequencer_0: node_index={}", test_setup.sequencer_0.node_index);
    info!("  - sequencer_1: node_index={}", test_setup.sequencer_1.node_index);

    // ==========================================================================
    // Step 2: Wait for nodes to be alive
    // ==========================================================================
    info!("\n--- Step 2: Waiting for nodes to be alive ---");
    info!("(Bootstrap runs synchronously during startup, so alive = bootstrap complete)");

    // Wait for both nodes to be alive
    let (result_0, result_1) = tokio::join!(
        test_setup
            .sequencer_0
            .monitoring_client
            .await_alive(AWAIT_ALIVE_INTERVAL_MS, AWAIT_ALIVE_ATTEMPTS),
        test_setup
            .sequencer_1
            .monitoring_client
            .await_alive(AWAIT_ALIVE_INTERVAL_MS, AWAIT_ALIVE_ATTEMPTS),
    );

    result_0.expect("Sequencer 0 should be alive after bootstrap");
    result_1.expect("Sequencer 1 should be alive after bootstrap");

    info!("Both nodes are alive - bootstrap phase completed");

    // ==========================================================================
    // Step 3: Verify block height
    // ==========================================================================
    info!("\n--- Step 3: Verifying block height ---");

    let height_0 = test_setup.sequencer_0.batcher_height().await;
    let height_1 = test_setup.sequencer_1.batcher_height().await;

    info!("  - sequencer_0 batcher height: {}", height_0);
    info!("  - sequencer_1 batcher height: {}", height_1);

    assert!(
        height_0 > BlockNumber(0),
        "Sequencer 0 should have committed at least one block during bootstrap, got {}",
        height_0
    );
    assert!(
        height_1 > BlockNumber(0),
        "Sequencer 1 should have committed at least one block during bootstrap, got {}",
        height_1
    );

    info!("Block height verification: PASS");

    // ==========================================================================
    // Step 4: Verify ERC20 balances
    // ==========================================================================
    info!("\n--- Step 4: Verifying ERC20 balances ---");

    let addresses = BootstrapAddresses::get();
    info!("Bootstrap addresses:");
    info!("  - funded_account: {:?}", addresses.funded_account_address);
    info!("  - eth_fee_token: {:?}", addresses.eth_fee_token_address);
    info!("  - strk_fee_token: {:?}", addresses.strk_fee_token_address);

    // Open storage for sequencer 0 to verify balances
    let storage_config = test_setup
        .sequencer_0
        .node_config
        .batcher_config
        .as_ref()
        .unwrap()
        .static_config
        .storage
        .clone();

    let (storage_reader, _storage_writer) =
        open_storage(storage_config).expect("Failed to open storage");

    // Check ETH balance
    let eth_balance = get_erc20_balance(
        &storage_reader,
        addresses.eth_fee_token_address,
        addresses.funded_account_address,
    );
    info!("  - ETH balance: {}", eth_balance);

    // Check STRK balance
    let strk_balance = get_erc20_balance(
        &storage_reader,
        addresses.strk_fee_token_address,
        addresses.funded_account_address,
    );
    info!("  - STRK balance: {}", strk_balance);

    assert!(
        eth_balance >= REQUIRED_BALANCE,
        "ETH balance {} should be >= required balance {}",
        eth_balance,
        REQUIRED_BALANCE
    );
    assert!(
        strk_balance >= REQUIRED_BALANCE,
        "STRK balance {} should be >= required balance {}",
        strk_balance,
        REQUIRED_BALANCE
    );

    info!("ERC20 balance verification: PASS");

    // ==========================================================================
    // Summary
    // ==========================================================================
    info!("\n===================================");
    info!("Bootstrap empty storage flow: ALL TESTS PASSED");
    info!("===================================");
    info!("");
    info!("What was verified:");
    info!("  1. Nodes started with empty storage");
    info!("  2. Bootstrap phase ran automatically before consensus");
    info!("  3. Blocks were committed (height > 0)");
    info!("  4. Funded account has sufficient ETH balance");
    info!("  5. Funded account has sufficient STRK balance");
    info!("");
    info!("Bootstrap empty storage integration test completed successfully!");
}
